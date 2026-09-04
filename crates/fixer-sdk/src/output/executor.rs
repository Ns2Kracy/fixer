//! Prepared output-plan execution with stale-state and overwrite protection.

use super::fingerprint::PathFingerprint;
use fixer_core::{CoreError, OutputOperation, OutputPlan};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use thiserror::Error;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverwritePolicy {
    #[default]
    NoOverwrite,
    Replace,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReflinkPolicy {
    #[default]
    Required,
    FallbackToCopy,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementMode {
    InPlace,
    RelativeSymlink,
    AbsoluteSymlink,
    Hardlink,
    Copy,
    Reflink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExecutionPolicy {
    dry_run: bool,
    overwrite: OverwritePolicy,
    reflink: ReflinkPolicy,
}
impl ExecutionPolicy {
    pub const fn dry_run() -> Self {
        Self {
            dry_run: true,
            overwrite: OverwritePolicy::NoOverwrite,
            reflink: ReflinkPolicy::Required,
        }
    }
    pub const fn with_overwrite(mut self, overwrite: OverwritePolicy) -> Self {
        self.overwrite = overwrite;
        self
    }
    pub const fn with_reflink(mut self, reflink: ReflinkPolicy) -> Self {
        self.reflink = reflink;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationStatus {
    DryRun,
    Completed,
    Reflinked,
    CopiedFallback,
    Failed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationReport {
    pub index: usize,
    pub target: PathBuf,
    pub status: OperationStatus,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionReport {
    operations: Vec<OperationReport>,
}
impl ExecutionReport {
    pub fn operations(&self) -> &[OperationReport] {
        &self.operations
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ExecutionError {
    #[error("unsafe output target `{path}`")]
    UnsafeTarget { path: PathBuf },
    #[error("output target already exists: `{path}`")]
    TargetExists { path: PathBuf },
    #[error("prepared output plan is stale at `{path}`")]
    StalePlan { path: PathBuf },
    #[error("source is unavailable: `{path}`")]
    SourceUnavailable { path: PathBuf },
    #[error("relative symlink cannot be represented from `{from}` to `{to}`")]
    RelativeSymlinkUnavailable { from: PathBuf, to: PathBuf },
    #[error("reflink is unsupported from `{source_path}` to `{target}`: {message}")]
    ReflinkUnsupported {
        source_path: PathBuf,
        target: PathBuf,
        message: String,
    },
    #[error("invalid output plan: {0}")]
    InvalidPlan(String),
    #[error("filesystem {action} failed for `{path}`: {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

#[derive(Debug, Error)]
#[error("{error}")]
pub struct ExecutionFailure {
    error: ExecutionError,
    report: ExecutionReport,
}
impl ExecutionFailure {
    pub const fn error(&self) -> &ExecutionError {
        &self.error
    }
    pub const fn report(&self) -> &ExecutionReport {
        &self.report
    }
}

#[derive(Debug, Clone)]
struct ObservedPath {
    path: PathBuf,
    fingerprint: PathFingerprint,
}

#[derive(Debug, Clone)]
pub struct PreparedOutputPlan {
    plan: OutputPlan,
    root: PathBuf,
    observed: Vec<ObservedPath>,
}
impl PreparedOutputPlan {
    pub const fn preview(&self) -> &OutputPlan {
        &self.plan
    }
    pub fn execute(&self, policy: ExecutionPolicy) -> Result<ExecutionReport, ExecutionFailure> {
        let mut report = ExecutionReport::default();
        if let Err(error) = self.ensure_fresh() {
            return Err(ExecutionFailure { error, report });
        }
        if !policy.dry_run
            && let Err((index, target, error)) = self.preflight(policy)
        {
            report.operations.push(OperationReport {
                index,
                target,
                status: OperationStatus::Failed,
            });
            return Err(ExecutionFailure { error, report });
        }
        for (index, operation) in self.plan.operations().iter().enumerate() {
            let target = absolute_target(&self.root, operation);
            if policy.dry_run {
                report.operations.push(OperationReport {
                    index,
                    target,
                    status: OperationStatus::DryRun,
                });
                continue;
            }
            if let Err(error) = ensure_safe_ancestors(&self.root, relative_target(operation)) {
                report.operations.push(OperationReport {
                    index,
                    target,
                    status: OperationStatus::Failed,
                });
                return Err(ExecutionFailure { error, report });
            }
            match execute_operation(operation, &self.root, policy) {
                Ok(status) => report.operations.push(OperationReport {
                    index,
                    target,
                    status,
                }),
                Err(error) => {
                    report.operations.push(OperationReport {
                        index,
                        target,
                        status: OperationStatus::Failed,
                    });
                    return Err(ExecutionFailure { error, report });
                }
            }
        }
        Ok(report)
    }
    fn preflight(&self, policy: ExecutionPolicy) -> Result<(), (usize, PathBuf, ExecutionError)> {
        for (index, operation) in self.plan.operations().iter().enumerate() {
            let target = absolute_target(&self.root, operation);
            let check = (|| {
                ensure_safe_ancestors(&self.root, relative_target(operation))?;
                match operation {
                    OutputOperation::CreateDirectory { .. } => {
                        if let Ok(metadata) = fs::symlink_metadata(&target)
                            && !metadata.is_dir()
                        {
                            return Err(ExecutionError::TargetExists {
                                path: target.clone(),
                            });
                        }
                    }
                    OutputOperation::WriteBytes { .. } => {
                        ensure_target_available(&target, policy.overwrite)?;
                    }
                    OutputOperation::Copy { .. }
                    | OutputOperation::Move { .. }
                    | OutputOperation::Symlink { .. }
                    | OutputOperation::Hardlink { .. }
                    | OutputOperation::Reflink { .. } => {
                        let source = required_source(operation, &self.root)?;
                        ensure_source_file(&source)?;
                        ensure_target_available(&target, policy.overwrite)?;
                    }
                }
                Ok(())
            })();
            if let Err(error) = check {
                return Err((index, target, error));
            }
        }
        Ok(())
    }

    fn ensure_fresh(&self) -> Result<(), ExecutionError> {
        for observed in &self.observed {
            let current = PathFingerprint::capture(&observed.path)
                .map_err(|source| io_error("fingerprint", &observed.path, source))?;
            if current != observed.fingerprint {
                return Err(ExecutionError::StalePlan {
                    path: observed.path.clone(),
                });
            }
        }
        Ok(())
    }
}

pub trait OutputPlanExt: Sized {
    fn prepare(self) -> Result<PreparedOutputPlan, ExecutionError>;
    fn preview(&self) -> Result<&OutputPlan, ExecutionError>;
    fn execute(self, policy: ExecutionPolicy) -> Result<ExecutionReport, ExecutionFailure>;
}
impl OutputPlanExt for OutputPlan {
    fn prepare(self) -> Result<PreparedOutputPlan, ExecutionError> {
        for operation in self.operations() {
            validate_operation(operation)?;
        }
        let root = absolute_path(&self.output_root)?;
        let mut paths = BTreeMap::<PathBuf, PathFingerprint>::new();
        for operation in self.operations() {
            let target = relative_target(operation);
            validate_relative_target(target)?;
            ensure_safe_ancestors(&root, target)?;
            let target = root.join(target);
            paths.insert(
                target.clone(),
                PathFingerprint::capture(&target)
                    .map_err(|source| io_error("fingerprint", &target, source))?,
            );
            if let Some(source) = resolved_source(operation, &root) {
                paths.insert(
                    source.clone(),
                    PathFingerprint::capture(&source)
                        .map_err(|error| io_error("fingerprint", &source, error))?,
                );
            }
        }
        let observed = paths
            .into_iter()
            .map(|(path, fingerprint)| ObservedPath { path, fingerprint })
            .collect();
        Ok(PreparedOutputPlan {
            plan: self,
            root,
            observed,
        })
    }

    fn preview(&self) -> Result<&OutputPlan, ExecutionError> {
        self.clone().prepare()?;
        Ok(self)
    }

    fn execute(self, policy: ExecutionPolicy) -> Result<ExecutionReport, ExecutionFailure> {
        match self.prepare() {
            Ok(prepared) => prepared.execute(policy),
            Err(error) => Err(ExecutionFailure {
                error,
                report: ExecutionReport::default(),
            }),
        }
    }
}

pub fn plan_media_placement(
    source: impl AsRef<Path>,
    output_root: impl AsRef<Path>,
    target: impl Into<PathBuf>,
    mode: PlacementMode,
) -> Result<OutputPlan, ExecutionError> {
    let source = source.as_ref();
    let output_root = output_root.as_ref();
    let target = target.into();
    let mut plan = OutputPlan::new(output_root);
    if mode == PlacementMode::InPlace {
        return Ok(plan);
    }
    let canonical_source = source
        .canonicalize()
        .map_err(|error| io_error("canonicalize", source, error))?;
    let operation = match mode {
        PlacementMode::InPlace => unreachable!(),
        PlacementMode::RelativeSymlink => {
            validate_relative_target(&target)?;
            let root = absolute_path(output_root)?;
            let parent = root
                .join(&target)
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| ExecutionError::UnsafeTarget {
                    path: target.clone(),
                })?;
            let relative = relative_path(&parent, &canonical_source).ok_or_else(|| {
                ExecutionError::RelativeSymlinkUnavailable {
                    from: parent,
                    to: canonical_source.clone(),
                }
            })?;
            OutputOperation::symlink(relative, target)
        }
        PlacementMode::AbsoluteSymlink => OutputOperation::symlink(canonical_source, target),
        PlacementMode::Hardlink => OutputOperation::hardlink(canonical_source, target),
        PlacementMode::Copy => OutputOperation::copy(canonical_source, target),
        PlacementMode::Reflink => OutputOperation::reflink(canonical_source, target),
    }
    .map_err(|error| core_error(&error))?;
    plan.push(operation);
    Ok(plan)
}

fn execute_operation(
    operation: &OutputOperation,
    root: &Path,
    policy: ExecutionPolicy,
) -> Result<OperationStatus, ExecutionError> {
    let target = absolute_target(root, operation);
    match operation {
        OutputOperation::CreateDirectory { .. } => {
            if let Ok(metadata) = fs::symlink_metadata(&target) {
                if metadata.is_dir() {
                    return Ok(OperationStatus::Completed);
                }
                return Err(ExecutionError::TargetExists { path: target });
            }
            fs::create_dir_all(&target)
                .map_err(|error| io_error("create directory", &target, error))?;
            Ok(OperationStatus::Completed)
        }
        OutputOperation::WriteBytes { content, .. } => {
            ensure_target_available(&target, policy.overwrite)?;
            let temp = write_temp(&target, content.as_bytes())?;
            publish_temp(&temp, &target, policy.overwrite)?;
            Ok(OperationStatus::Completed)
        }
        OutputOperation::Copy { .. } => {
            let source = required_source(operation, root)?;
            ensure_source_file(&source)?;
            ensure_target_available(&target, policy.overwrite)?;
            let temp = copy_temp(&source, &target)?;
            publish_temp(&temp, &target, policy.overwrite)?;
            Ok(OperationStatus::Completed)
        }
        OutputOperation::Move { .. } => execute_move(operation, root, &target, policy.overwrite),
        OutputOperation::Symlink { source, .. } => {
            let resolved = required_source(operation, root)?;
            ensure_source_file(&resolved)?;
            ensure_parent(&target)?;
            ensure_target_available(&target, policy.overwrite)?;
            if policy.overwrite == OverwritePolicy::Replace {
                let temp = unique_temp_path(&target)?;
                create_symlink(source, &temp)
                    .map_err(|error| map_target_io("create temporary symlink", &temp, error))?;
                publish_replacement(&temp, &target)?;
            } else {
                create_symlink(source, &target)
                    .map_err(|error| map_target_io("create symlink", &target, error))?;
            }
            Ok(OperationStatus::Completed)
        }
        OutputOperation::Hardlink { .. } => {
            let source = required_source(operation, root)?;
            ensure_source_file(&source)?;
            ensure_parent(&target)?;
            ensure_target_available(&target, policy.overwrite)?;
            if policy.overwrite == OverwritePolicy::Replace {
                let temp = unique_temp_path(&target)?;
                fs::hard_link(&source, &temp)
                    .map_err(|error| map_target_io("create temporary hardlink", &temp, error))?;
                publish_replacement(&temp, &target)?;
            } else {
                fs::hard_link(&source, &target)
                    .map_err(|error| map_target_io("create hardlink", &target, error))?;
            }
            Ok(OperationStatus::Completed)
        }
        OutputOperation::Reflink { .. } => execute_reflink(operation, root, &target, policy),
    }
}

fn execute_move(
    operation: &OutputOperation,
    root: &Path,
    target: &Path,
    overwrite: OverwritePolicy,
) -> Result<OperationStatus, ExecutionError> {
    let source = required_source(operation, root)?;
    ensure_source_file(&source)?;
    ensure_parent(target)?;
    ensure_target_available(target, overwrite)?;
    execute_move_after_validation(&source, target, overwrite, publish_move_direct)?;
    Ok(OperationStatus::Completed)
}

fn execute_move_after_validation(
    source: &Path,
    target: &Path,
    overwrite: OverwritePolicy,
    direct_move: impl FnOnce(&Path, &Path, OverwritePolicy) -> io::Result<()>,
) -> Result<(), ExecutionError> {
    execute_move_after_validation_with(
        source,
        target,
        overwrite,
        direct_move,
        copy_temp,
        publish_copied_move,
    )
}

fn execute_move_after_validation_with<DirectMove, CopyTemp, PublishCopiedMove>(
    source: &Path,
    target: &Path,
    overwrite: OverwritePolicy,
    direct_move: DirectMove,
    copy_to_temp: CopyTemp,
    publish_copy: PublishCopiedMove,
) -> Result<(), ExecutionError>
where
    DirectMove: FnOnce(&Path, &Path, OverwritePolicy) -> io::Result<()>,
    CopyTemp: FnOnce(&Path, &Path) -> Result<PathBuf, ExecutionError>,
    PublishCopiedMove: FnOnce(&Path, &Path, OverwritePolicy) -> Result<(), ExecutionError>,
{
    let source_fingerprint = capture_move_source(source)?;
    match direct_move(source, target, overwrite) {
        Ok(()) => finish_direct_move(source, target, &source_fingerprint),
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            ensure_source_fresh(source, &source_fingerprint)?;
            let temp = copy_to_temp(source, target)?;
            if let Err(error) = ensure_source_fresh(source, &source_fingerprint) {
                let _ = fs::remove_file(&temp);
                return Err(error);
            }
            if let Err(error) = publish_copy(&temp, target, overwrite) {
                let _ = fs::remove_file(&temp);
                return Err(error);
            }
            ensure_source_fresh(source, &source_fingerprint)?;
            remove_moved_source(source)
        }
        Err(error) => Err(map_target_io("move file", target, error)),
    }
}

fn capture_move_source(source: &Path) -> Result<PathFingerprint, ExecutionError> {
    let fingerprint = PathFingerprint::capture(source)
        .map_err(|error| io_error("fingerprint move source", source, error))?;
    if fingerprint == PathFingerprint::Missing {
        Err(ExecutionError::SourceUnavailable {
            path: source.to_path_buf(),
        })
    } else {
        Ok(fingerprint)
    }
}

fn ensure_source_fresh(source: &Path, expected: &PathFingerprint) -> Result<(), ExecutionError> {
    let current = PathFingerprint::capture(source)
        .map_err(|error| io_error("fingerprint move source", source, error))?;
    if &current == expected {
        Ok(())
    } else {
        Err(ExecutionError::StalePlan {
            path: source.to_path_buf(),
        })
    }
}

fn finish_direct_move(
    source: &Path,
    target: &Path,
    source_fingerprint: &PathFingerprint,
) -> Result<(), ExecutionError> {
    if source == target
        || matches!(
            (source.canonicalize(), target.canonicalize()),
            (Ok(source), Ok(target)) if source == target
        )
    {
        return Ok(());
    }
    let current = PathFingerprint::capture(source)
        .map_err(|error| io_error("fingerprint moved source", source, error))?;
    if current == PathFingerprint::Missing {
        return Ok(());
    }
    let published = PathFingerprint::capture(target)
        .map_err(|error| io_error("fingerprint moved target", target, error))?;
    if &current != source_fingerprint || !current.same_file_identity(&published) {
        return Err(ExecutionError::StalePlan {
            path: source.to_path_buf(),
        });
    }
    remove_moved_source(source)
}

fn publish_move_direct(source: &Path, target: &Path, overwrite: OverwritePolicy) -> io::Result<()> {
    match overwrite {
        OverwritePolicy::NoOverwrite => publish_move_no_overwrite(source, target),
        OverwritePolicy::Replace => publish_move_replacement(source, target),
    }
}

#[cfg(not(windows))]
fn publish_move_no_overwrite(source: &Path, target: &Path) -> io::Result<()> {
    publish_move_no_overwrite_with(
        source,
        target,
        reserve_move_target,
        |source, target| fs::rename(source, target),
        |target| fs::remove_file(target),
    )
}

#[cfg(windows)]
fn publish_move_no_overwrite(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

fn reserve_move_target(target: &Path) -> io::Result<PathFingerprint> {
    let reservation = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    reservation
        .metadata()
        .map(|metadata| PathFingerprint::from_metadata(&metadata))
}

fn publish_move_no_overwrite_with<Reserve, Rename, Cleanup>(
    source: &Path,
    target: &Path,
    reserve: Reserve,
    rename: Rename,
    cleanup: Cleanup,
) -> io::Result<()>
where
    Reserve: FnOnce(&Path) -> io::Result<PathFingerprint>,
    Rename: FnOnce(&Path, &Path) -> io::Result<()>,
    Cleanup: FnOnce(&Path) -> io::Result<()>,
{
    let reservation = reserve(target)?;
    if PathFingerprint::capture(target)? != reservation {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "move target reservation changed",
        ));
    }
    if let Err(rename_error) = rename(source, target) {
        match PathFingerprint::capture(target)? {
            current if current == reservation => cleanup(target)?,
            PathFingerprint::Missing => {}
            PathFingerprint::Present { .. } => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "move target reservation changed",
                ));
            }
        }
        return Err(rename_error);
    }
    Ok(())
}

#[cfg(not(windows))]
fn publish_move_replacement(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

#[cfg(windows)]
fn publish_move_replacement(source: &Path, target: &Path) -> io::Result<()> {
    replace_existing_on_windows_with(source, target, fs::rename, fs::remove_file)
}

fn publish_copied_move(
    temp: &Path,
    target: &Path,
    overwrite: OverwritePolicy,
) -> Result<(), ExecutionError> {
    let result = match overwrite {
        OverwritePolicy::NoOverwrite => publish_move_no_overwrite(temp, target)
            .map_err(|error| map_target_io("publish copied move", target, error)),
        OverwritePolicy::Replace => publish_replacement(temp, target),
    };
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(any(test, windows))]
fn replace_existing_on_windows_with(
    source: &Path,
    target: &Path,
    mut rename: impl FnMut(&Path, &Path) -> io::Result<()>,
    mut remove: impl FnMut(&Path) -> io::Result<()>,
) -> io::Result<()> {
    rename(source, target).or_else(|first_error| {
        if matches!(
            first_error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::PermissionDenied
        ) {
            remove(target)?;
            rename(source, target)
        } else {
            Err(first_error)
        }
    })
}

fn remove_moved_source(source: &Path) -> Result<(), ExecutionError> {
    fs::remove_file(source).map_err(|error| io_error("remove moved source", source, error))
}

fn execute_reflink(
    operation: &OutputOperation,
    root: &Path,
    target: &Path,
    policy: ExecutionPolicy,
) -> Result<OperationStatus, ExecutionError> {
    let source = required_source(operation, root)?;
    ensure_source_file(&source)?;
    ensure_parent(target)?;
    ensure_target_available(target, policy.overwrite)?;
    let temp = unique_temp_path(target)?;
    match reflink_copy::reflink(&source, &temp) {
        Ok(()) => {
            publish_temp(&temp, target, policy.overwrite)?;
            Ok(OperationStatus::Reflinked)
        }
        Err(error) => {
            let kind = error.kind();
            if kind != io::ErrorKind::AlreadyExists {
                let _ = fs::remove_file(&temp);
            }
            match kind {
                io::ErrorKind::NotFound => {
                    return Err(ExecutionError::SourceUnavailable { path: source });
                }
                io::ErrorKind::PermissionDenied => {
                    return Err(io_error("reflink", target, error));
                }
                io::ErrorKind::AlreadyExists => {
                    return Err(io_error("reserve reflink temporary file", &temp, error));
                }
                _ => {}
            }
            if policy.reflink == ReflinkPolicy::Required {
                return Err(ExecutionError::ReflinkUnsupported {
                    source_path: source,
                    target: target.to_path_buf(),
                    message: error.to_string(),
                });
            }
            let temp = copy_temp(&source, target)?;
            publish_temp(&temp, target, policy.overwrite)?;
            Ok(OperationStatus::CopiedFallback)
        }
    }
}

fn write_temp(target: &Path, content: &[u8]) -> Result<PathBuf, ExecutionError> {
    ensure_parent(target)?;
    let temp = unique_temp_path(target)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| io_error("create temporary file", &temp, error))?;
    let result = file
        .write_all(content)
        .map_err(|error| io_error("write temporary file", &temp, error))
        .and_then(|()| {
            file.sync_all()
                .map_err(|error| io_error("sync temporary file", &temp, error))
        });
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(temp)
}

fn copy_temp(source: &Path, target: &Path) -> Result<PathBuf, ExecutionError> {
    ensure_parent(target)?;
    let temp = unique_temp_path(target)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| io_error("reserve temporary file", &temp, error))?;
    if let Err(error) = fs::copy(source, &temp) {
        let _ = fs::remove_file(&temp);
        return Err(io_error("copy to temporary file", source, error));
    }
    Ok(temp)
}

fn publish_temp(
    temp: &Path,
    target: &Path,
    overwrite: OverwritePolicy,
) -> Result<(), ExecutionError> {
    let result = match overwrite {
        OverwritePolicy::NoOverwrite => fs::hard_link(temp, target)
            .map_err(|error| map_target_io("publish temporary file", target, error))
            .and_then(|()| {
                fs::remove_file(temp)
                    .map_err(|error| io_error("remove temporary file", temp, error))
            }),
        OverwritePolicy::Replace => publish_replacement(temp, target),
    };
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(not(windows))]
fn publish_replacement(temp: &Path, target: &Path) -> Result<(), ExecutionError> {
    let result =
        fs::rename(temp, target).map_err(|error| io_error("replace target", target, error));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(windows)]
fn publish_replacement(temp: &Path, target: &Path) -> Result<(), ExecutionError> {
    let result = replace_existing_on_windows_with(temp, target, fs::rename, fs::remove_file)
        .map_err(|error| io_error("replace target", target, error));
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn ensure_parent(target: &Path) -> Result<(), ExecutionError> {
    let parent = target
        .parent()
        .ok_or_else(|| ExecutionError::UnsafeTarget {
            path: target.to_path_buf(),
        })?;
    fs::create_dir_all(parent).map_err(|error| io_error("create parent directory", parent, error))
}
fn ensure_target_available(
    target: &Path,
    overwrite: OverwritePolicy,
) -> Result<(), ExecutionError> {
    if overwrite == OverwritePolicy::NoOverwrite && fs::symlink_metadata(target).is_ok() {
        Err(ExecutionError::TargetExists {
            path: target.to_path_buf(),
        })
    } else {
        Ok(())
    }
}
fn ensure_source_file(source: &Path) -> Result<(), ExecutionError> {
    match fs::metadata(source) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        _ => Err(ExecutionError::SourceUnavailable {
            path: source.to_path_buf(),
        }),
    }
}
fn required_source(operation: &OutputOperation, root: &Path) -> Result<PathBuf, ExecutionError> {
    resolved_source(operation, root)
        .ok_or_else(|| ExecutionError::InvalidPlan("operation has no source".to_owned()))
}
fn resolved_source(operation: &OutputOperation, root: &Path) -> Option<PathBuf> {
    let source = operation.source()?;
    if matches!(operation, OutputOperation::Symlink { .. }) && source.is_relative() {
        return absolute_target(root, operation)
            .parent()
            .map(|parent| normalize_path(&parent.join(source)));
    }
    if source.is_absolute() {
        Some(source.to_path_buf())
    } else {
        std::env::current_dir()
            .ok()
            .map(|directory| directory.join(source))
    }
}
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

fn relative_target(operation: &OutputOperation) -> &Path {
    operation
        .target()
        .expect("all output operations have a target")
}
fn absolute_target(root: &Path, operation: &OutputOperation) -> PathBuf {
    root.join(relative_target(operation))
}

fn validate_operation(operation: &OutputOperation) -> Result<(), ExecutionError> {
    validate_relative_target(relative_target(operation))?;
    if let OutputOperation::Move { source, .. } = operation
        && !source.is_absolute()
    {
        return Err(ExecutionError::InvalidPlan(format!(
            "move source must be absolute: `{}`",
            source.display()
        )));
    }
    Ok(())
}

fn validate_relative_target(target: &Path) -> Result<(), ExecutionError> {
    if target.as_os_str().is_empty()
        || target.is_absolute()
        || target.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
        || target.to_string_lossy().contains('\0')
    {
        return Err(ExecutionError::UnsafeTarget {
            path: target.to_path_buf(),
        });
    }
    Ok(())
}
fn ensure_safe_ancestors(root: &Path, target: &Path) -> Result<(), ExecutionError> {
    validate_relative_target(target)?;
    if fs::symlink_metadata(root).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(ExecutionError::UnsafeTarget {
            path: root.to_path_buf(),
        });
    }
    let mut current = root.to_path_buf();
    if let Some(parent) = target.parent() {
        for component in parent.components() {
            let Component::Normal(segment) = component else {
                continue;
            };
            current.push(segment);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(ExecutionError::UnsafeTarget { path: current });
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => break,
                Err(error) => return Err(io_error("inspect output ancestor", &current, error)),
            }
        }
    }
    Ok(())
}
fn absolute_path(path: &Path) -> Result<PathBuf, ExecutionError> {
    if path.as_os_str().is_empty() {
        return Err(ExecutionError::UnsafeTarget {
            path: path.to_path_buf(),
        });
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .map_err(|error| io_error("resolve path", path, error))?
    };
    canonicalize_existing_ancestor(&absolute)
}

fn canonicalize_existing_ancestor(path: &Path) -> Result<PathBuf, ExecutionError> {
    let mut existing = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match existing.canonicalize() {
            Ok(mut canonical) => {
                for component in missing.iter().rev() {
                    canonical.push(component);
                }
                return Ok(canonical);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let Some(component) = existing.file_name().map(OsString::from) else {
                    return Err(io_error("canonicalize output root", path, error));
                };
                missing.push(component);
                existing.pop();
            }
            Err(error) => return Err(io_error("canonicalize output root", path, error)),
        }
    }
}
fn unique_temp_path(target: &Path) -> Result<PathBuf, ExecutionError> {
    let parent = target
        .parent()
        .ok_or_else(|| ExecutionError::UnsafeTarget {
            path: target.to_path_buf(),
        })?;
    let name = target.file_name().unwrap_or_default();
    for _ in 0..100 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let mut temp_name = OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".fixer-tmp-{}-{sequence}", std::process::id()));
        let path = parent.join(temp_name);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(path),
            Ok(_) => {}
            Err(error) => return Err(io_error("inspect temporary path", &path, error)),
        }
    }
    Err(ExecutionError::InvalidPlan(
        "could not reserve a temporary output path".to_owned(),
    ))
}
fn relative_path(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_parts = from.components().collect::<Vec<_>>();
    let to_parts = to.components().collect::<Vec<_>>();
    if from_parts.first().and_then(prefix) != to_parts.first().and_then(prefix) {
        return None;
    }
    let mut common = 0;
    while common < from_parts.len()
        && common < to_parts.len()
        && from_parts[common] == to_parts[common]
    {
        common += 1;
    }
    let mut result = PathBuf::new();
    for component in &from_parts[common..] {
        if matches!(component, Component::Normal(_)) {
            result.push("..");
        }
    }
    for component in &to_parts[common..] {
        result.push(component.as_os_str());
    }
    Some(result)
}
fn prefix(component: &Component<'_>) -> Option<OsString> {
    match component {
        Component::Prefix(value) => Some(value.as_os_str().to_owned()),
        Component::RootDir => Some(OsString::from(std::path::MAIN_SEPARATOR.to_string())),
        _ => None,
    }
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}
#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(source, target)
}
#[cfg(not(any(unix, windows)))]
fn create_symlink(_source: &Path, _target: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "file symlinks are unsupported",
    ))
}

fn core_error(error: &CoreError) -> ExecutionError {
    ExecutionError::InvalidPlan(error.to_string())
}
fn map_target_io(action: &'static str, path: &Path, source: io::Error) -> ExecutionError {
    if source.kind() == io::ErrorKind::AlreadyExists {
        ExecutionError::TargetExists {
            path: path.to_path_buf(),
        }
    } else {
        io_error(action, path, source)
    }
}
fn io_error(action: &'static str, path: &Path, source: io::Error) -> ExecutionError {
    ExecutionError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExecutionError, OverwritePolicy, copy_temp, execute_move_after_validation,
        execute_move_after_validation_with, publish_copied_move, publish_move_no_overwrite_with,
        replace_existing_on_windows_with, reserve_move_target,
    };
    use std::{cell::Cell, fs, io};

    #[test]
    fn move_windows_replace_retries_after_removing_an_existing_target() {
        let mut rename_attempts = 0;
        let mut removed = false;

        replace_existing_on_windows_with(
            std::path::Path::new("source.mkv"),
            std::path::Path::new("target.mkv"),
            |_, _| {
                rename_attempts += 1;
                if rename_attempts == 1 {
                    Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "test collision",
                    ))
                } else {
                    Ok(())
                }
            },
            |_| {
                removed = true;
                Ok(())
            },
        )
        .unwrap();

        assert_eq!(rename_attempts, 2);
        assert!(removed);
    }

    #[test]
    fn move_cross_device_fallback_publishes_then_removes_source() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("library/movie.mkv");
        fs::write(&source, [0, 1, 2, 255]).unwrap();

        execute_move_after_validation(&source, &target, OverwritePolicy::NoOverwrite, |_, _, _| {
            Err(io::Error::new(
                io::ErrorKind::CrossesDevices,
                "test boundary",
            ))
        })
        .unwrap();

        assert_eq!(fs::read(&target).unwrap(), [0, 1, 2, 255]);
        assert!(!source.exists());
    }

    #[test]
    fn move_cross_device_publication_failure_preserves_source_and_cleans_temp() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("library/movie.mkv");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&source, b"incoming").unwrap();
        fs::write(&target, b"existing").unwrap();

        let error = execute_move_after_validation(
            &source,
            &target,
            OverwritePolicy::NoOverwrite,
            |_, _, _| {
                Err(io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    "test boundary",
                ))
            },
        )
        .unwrap_err();

        assert!(matches!(error, ExecutionError::TargetExists { .. }));
        assert_eq!(fs::read(&source).unwrap(), b"incoming");
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        assert!(
            fs::read_dir(target.parent().unwrap())
                .unwrap()
                .all(|entry| {
                    !entry
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .contains(".fixer-tmp")
                })
        );
    }

    #[test]
    fn move_cross_device_change_after_copy_is_stale_and_unpublished() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("library/movie.mkv");
        fs::write(&source, b"original").unwrap();

        let error = execute_move_after_validation_with(
            &source,
            &target,
            OverwritePolicy::NoOverwrite,
            |_, _, _| {
                Err(io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    "test boundary",
                ))
            },
            |source, target| {
                let temp = copy_temp(source, target)?;
                fs::write(source, b"changed after copy")
                    .map_err(|error| super::io_error("change test source", source, error))?;
                Ok(temp)
            },
            publish_copied_move,
        )
        .unwrap_err();

        assert!(matches!(error, ExecutionError::StalePlan { path } if path == source));
        assert_eq!(fs::read(&source).unwrap(), b"changed after copy");
        assert!(!target.exists());
        assert!(
            fs::read_dir(target.parent().unwrap())
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".fixer-tmp"))
        );
    }

    #[test]
    fn move_cross_device_replacement_after_publication_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("library/movie.mkv");
        fs::write(&source, b"original").unwrap();
        let source_to_replace = source.clone();

        let error = execute_move_after_validation_with(
            &source,
            &target,
            OverwritePolicy::NoOverwrite,
            |_, _, _| {
                Err(io::Error::new(
                    io::ErrorKind::CrossesDevices,
                    "test boundary",
                ))
            },
            copy_temp,
            move |temp, target, overwrite| {
                publish_copied_move(temp, target, overwrite)?;
                fs::remove_file(&source_to_replace).map_err(|error| {
                    super::io_error("replace test source", &source_to_replace, error)
                })?;
                fs::write(&source_to_replace, b"new replacement").map_err(|error| {
                    super::io_error("replace test source", &source_to_replace, error)
                })
            },
        )
        .unwrap_err();

        assert!(matches!(error, ExecutionError::StalePlan { path } if path == source));
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&source).unwrap(), b"new replacement");
    }

    #[cfg(unix)]
    #[test]
    fn move_replace_does_not_remove_a_different_source_replacement() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("target.mkv");
        fs::write(&source, b"original").unwrap();
        fs::hard_link(&source, &target).unwrap();

        let error = execute_move_after_validation_with(
            &source,
            &target,
            OverwritePolicy::Replace,
            |source, target, _| {
                fs::rename(source, target)?;
                fs::remove_file(source)?;
                fs::write(source, b"replacement")
            },
            copy_temp,
            publish_copied_move,
        )
        .unwrap_err();

        assert!(matches!(error, ExecutionError::StalePlan { path } if path == source));
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(fs::read(&source).unwrap(), b"replacement");
    }

    #[cfg(not(windows))]
    #[test]
    fn move_no_overwrite_reserves_then_renames_without_hardlinking() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("target.mkv");
        fs::write(&source, b"incoming").unwrap();
        let reserve_calls = Cell::new(0);
        let rename_calls = Cell::new(0);

        publish_move_no_overwrite_with(
            &source,
            &target,
            |target| {
                reserve_calls.set(reserve_calls.get() + 1);
                reserve_move_target(target)
            },
            |source, target| {
                rename_calls.set(rename_calls.get() + 1);
                fs::rename(source, target)
            },
            |target| fs::remove_file(target),
        )
        .unwrap();

        assert_eq!(reserve_calls.get(), 1);
        assert_eq!(rename_calls.get(), 1);
        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"incoming");
    }

    #[cfg(not(windows))]
    #[test]
    fn move_no_overwrite_does_not_rename_over_a_changed_reservation() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.mkv");
        let target = root.path().join("target.mkv");
        fs::write(&source, b"incoming").unwrap();
        let rename_calls = Cell::new(0);
        let cleanup_calls = Cell::new(0);

        let error = publish_move_no_overwrite_with(
            &source,
            &target,
            |target| {
                let reservation = reserve_move_target(target)?;
                fs::remove_file(target)?;
                fs::write(target, b"collision")?;
                Ok(reservation)
            },
            |source, target| {
                rename_calls.set(rename_calls.get() + 1);
                fs::rename(source, target)
            },
            |target| {
                cleanup_calls.set(cleanup_calls.get() + 1);
                fs::remove_file(target)
            },
        )
        .unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(rename_calls.get(), 0);
        assert_eq!(cleanup_calls.get(), 0);
        assert_eq!(fs::read(&source).unwrap(), b"incoming");
        assert_eq!(fs::read(&target).unwrap(), b"collision");
    }

    #[test]
    fn move_does_not_fallback_for_other_direct_errors() {
        for kind in [io::ErrorKind::PermissionDenied, io::ErrorKind::Unsupported] {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source.mkv");
            let target = root.path().join("library/movie.mkv");
            fs::write(&source, b"incoming").unwrap();

            let error = execute_move_after_validation(
                &source,
                &target,
                OverwritePolicy::NoOverwrite,
                |_, _, _| Err(io::Error::new(kind, "test failure")),
            )
            .unwrap_err();

            assert!(matches!(
                error,
                ExecutionError::Io { source, .. } if source.kind() == kind
            ));
            assert_eq!(fs::read(&source).unwrap(), b"incoming");
            assert!(!target.exists());
        }
    }
}
