//! Destination layout and placement planning shared by CLI and server jobs.

use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};

use fixer_core::{
    AnimeSeries, BookWork, CoreError, CreditRole, Movie, MusicReleaseGroup, OutputOperation,
    OutputPlan, PlannedContent, Resolved, Series,
};
use thiserror::Error;

use crate::{PathTemplate, TemplateContext, TemplateError};

/// A filesystem operation selected for one source media file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizationPlacement {
    Move,
    Copy,
    Hardlink,
    Symlink,
    Reflink,
}

/// Resolved metadata used to select a built-in destination layout.
#[derive(Debug, Clone, Copy)]
pub enum OrganizationMedia<'a> {
    Movie(&'a Resolved<Movie>),
    Television(&'a Resolved<Series>),
    Anime(&'a Resolved<AnimeSeries>),
    Music(&'a Resolved<MusicReleaseGroup>),
    Book(&'a Resolved<BookWork>),
}

/// Inputs needed to build a plan rooted at one destination directory.
#[derive(Debug)]
pub struct OrganizationRequest<'a> {
    source_path: &'a Path,
    destination_path: &'a Path,
    media: OrganizationMedia<'a>,
    placement: OrganizationPlacement,
    path_template: Option<&'a str>,
    media_target: Option<&'a Path>,
    metadata_plan: OutputPlan,
}

impl<'a> OrganizationRequest<'a> {
    pub fn new(
        source_path: &'a Path,
        destination_path: &'a Path,
        media: OrganizationMedia<'a>,
        placement: OrganizationPlacement,
        metadata_plan: OutputPlan,
    ) -> Self {
        Self {
            source_path,
            destination_path,
            media,
            placement,
            path_template: None,
            media_target: None,
            metadata_plan,
        }
    }

    pub const fn with_path_template(mut self, path_template: &'a str) -> Self {
        self.path_template = Some(path_template);
        self
    }

    /// Overrides the media path beneath the rendered package directory.
    pub const fn with_media_target(mut self, media_target: &'a Path) -> Self {
        self.media_target = Some(media_target);
        self
    }
}

/// A rejected organization template, layout, or output operation.
#[derive(Debug, Error)]
pub enum OrganizationError {
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Core(#[from] CoreError),
    #[error("resolved {0} metadata has no title")]
    MissingTitle(&'static str),
    #[error("source media path has no file name: `{0}`")]
    MissingFileName(PathBuf),
    #[error("source media path has no extension: `{0}`")]
    MissingExtension(PathBuf),
    #[error("organization path has no final component: `{0}`")]
    MissingTargetName(PathBuf),
    #[error("relative symlink cannot be represented from `{from}` to `{to}`")]
    RelativeSymlinkUnavailable { from: PathBuf, to: PathBuf },
    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(std::io::Error),
    #[error("invalid planned manifest: {0}")]
    InvalidManifest(serde_json::Error),
    #[error("could not serialize planned manifest: {0}")]
    SerializeManifest(serde_json::Error),
}

/// Builds a single bounded plan whose targets are relative to `destination_path`.
pub fn organize(request: OrganizationRequest<'_>) -> Result<OutputPlan, OrganizationError> {
    let package = package_path(request.media, request.path_template)?;
    let media_target = request.media_target.map_or_else(
        || media_target(request.source_path, request.media, &package),
        |target| Ok(package.join(target)),
    )?;
    let mut plan = OutputPlan::new(request.destination_path);
    plan.push(placement_operation(
        request.source_path,
        request.destination_path,
        media_target,
        request.placement,
    )?);
    for operation in request.metadata_plan.operations() {
        plan.push(rebase_operation(operation, &package)?);
    }
    Ok(plan)
}

/// Removes writer-declared asset transfers while keeping manifests truthful.
pub fn metadata_only(plan: OutputPlan) -> Result<OutputPlan, OrganizationError> {
    let dropped_targets = plan
        .operations()
        .iter()
        .filter(|operation| {
            !matches!(
                operation,
                OutputOperation::CreateDirectory { .. } | OutputOperation::WriteBytes { .. }
            )
        })
        .filter_map(OutputOperation::target)
        .map(Path::to_path_buf)
        .collect::<Vec<_>>();
    let mut filtered = OutputPlan::new(plan.output_root.clone());
    for operation in plan.operations() {
        match operation {
            OutputOperation::CreateDirectory { .. } => filtered.push(operation.clone()),
            OutputOperation::WriteBytes { target, content }
                if target.file_name().and_then(|name| name.to_str())
                    == Some("fixer-manifest.json") =>
            {
                filtered.push(reconcile_manifest(target, content, &dropped_targets)?);
            }
            OutputOperation::WriteBytes { .. } => filtered.push(operation.clone()),
            OutputOperation::Copy { .. }
            | OutputOperation::Move { .. }
            | OutputOperation::Symlink { .. }
            | OutputOperation::Hardlink { .. }
            | OutputOperation::Reflink { .. } => {}
        }
    }
    Ok(filtered)
}

fn package_path(
    media: OrganizationMedia<'_>,
    path_template: Option<&str>,
) -> Result<PathBuf, OrganizationError> {
    let (context, built_in) = match media {
        OrganizationMedia::Movie(resolved) => {
            let context = TemplateContext::movie(resolved, ["zh-CN", "en", "und"])?;
            let built_in = if resolved.value.release_year().is_some() {
                "{{ title | sanitize }} ({{ year }})"
            } else {
                "{{ title | sanitize }}"
            };
            (context, PathBuf::from(built_in))
        }
        OrganizationMedia::Television(resolved) => (
            basic_context(
                "television",
                &resolved.value.titles,
                resolved.value.id.as_str(),
            )?,
            PathBuf::from("{{ title | sanitize }}"),
        ),
        OrganizationMedia::Anime(resolved) => (
            basic_context("anime", &resolved.value.titles, resolved.value.id.as_str())?,
            PathBuf::from("{{ title | sanitize }}"),
        ),
        OrganizationMedia::Music(resolved) => {
            let context =
                basic_context("music", &resolved.value.titles, resolved.value.id.as_str())?;
            let artist = safe_folder_name(&resolved.value.artist.name, "Unknown Artist");
            (
                context,
                PathBuf::from(artist).join("{{ title | sanitize }}"),
            )
        }
        OrganizationMedia::Book(resolved) => {
            let context =
                basic_context("book", &resolved.value.titles, resolved.value.id.as_str())?;
            let author = resolved
                .value
                .contributors
                .iter()
                .find(|credit| credit.role == CreditRole::Author)
                .map_or("Unknown Author", |credit| credit.person.name.as_str());
            (
                context,
                PathBuf::from(safe_folder_name(author, "Unknown Author"))
                    .join("{{ title | sanitize }}"),
            )
        }
    };
    let source =
        path_template.map_or_else(|| built_in.to_string_lossy().into_owned(), str::to_owned);
    PathTemplate::new(source)
        .and_then(|template| template.render(&context))
        .map_err(Into::into)
}

fn basic_context(
    kind: &'static str,
    titles: &fixer_core::Titles,
    id: &str,
) -> Result<TemplateContext, OrganizationError> {
    let title = titles
        .entries()
        .first()
        .map(|entry| entry.value().clone())
        .ok_or(OrganizationError::MissingTitle(kind))?;
    TemplateContext::preview(title, id, None, None).map_err(Into::into)
}

fn media_target(
    source: &Path,
    media: OrganizationMedia<'_>,
    package: &Path,
) -> Result<PathBuf, OrganizationError> {
    let file_name = source
        .file_name()
        .ok_or_else(|| OrganizationError::MissingFileName(source.to_owned()))?;
    let extension = source
        .extension()
        .ok_or_else(|| OrganizationError::MissingExtension(source.to_owned()))?;
    let target = match media {
        OrganizationMedia::Movie(_) | OrganizationMedia::Book(_) => {
            let stem = package
                .file_name()
                .ok_or_else(|| OrganizationError::MissingTargetName(package.to_owned()))?;
            let mut file = PathBuf::from(stem);
            file.set_extension(extension);
            package.join(file)
        }
        OrganizationMedia::Television(resolved) => {
            let season = resolved
                .value
                .seasons
                .first()
                .map_or(0, |season| season.number);
            package.join(format!("Season {season:02}")).join(file_name)
        }
        OrganizationMedia::Anime(resolved) => {
            let cour = resolved.value.cours.first().map_or(1, |cour| cour.number);
            package.join(format!("Cour {cour:02}")).join(file_name)
        }
        OrganizationMedia::Music(_) => package.join(file_name),
    };
    Ok(target)
}

fn placement_operation(
    source: &Path,
    destination: &Path,
    target: PathBuf,
    placement: OrganizationPlacement,
) -> Result<OutputOperation, OrganizationError> {
    let operation = match placement {
        OrganizationPlacement::Move => OutputOperation::move_file(source, target),
        OrganizationPlacement::Copy => OutputOperation::copy(source, target),
        OrganizationPlacement::Hardlink => OutputOperation::hardlink(source, target),
        OrganizationPlacement::Symlink => {
            let destination = absolute_path(destination)?;
            let parent = destination
                .join(&target)
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| OrganizationError::MissingTargetName(target.clone()))?;
            let source = relative_path(&parent, source).ok_or_else(|| {
                OrganizationError::RelativeSymlinkUnavailable {
                    from: parent,
                    to: source.to_owned(),
                }
            })?;
            OutputOperation::symlink(source, target)
        }
        OrganizationPlacement::Reflink => OutputOperation::reflink(source, target),
    };
    operation.map_err(Into::into)
}

fn rebase_operation(
    operation: &OutputOperation,
    package: &Path,
) -> Result<OutputOperation, CoreError> {
    let target = package.join(
        operation
            .target()
            .expect("all output operations have a target"),
    );
    match operation {
        OutputOperation::CreateDirectory { .. } => OutputOperation::create_directory(target),
        OutputOperation::WriteBytes { content, .. } => {
            OutputOperation::write_bytes(target, content.clone())
        }
        OutputOperation::Copy { source, .. } => OutputOperation::copy(source, target),
        OutputOperation::Move { source, .. } => OutputOperation::move_file(source, target),
        OutputOperation::Symlink { source, .. } => OutputOperation::symlink(source, target),
        OutputOperation::Hardlink { source, .. } => OutputOperation::hardlink(source, target),
        OutputOperation::Reflink { source, .. } => OutputOperation::reflink(source, target),
    }
}

fn reconcile_manifest(
    target: &Path,
    content: &PlannedContent,
    dropped_targets: &[PathBuf],
) -> Result<OutputOperation, OrganizationError> {
    let mut manifest: serde_json::Value =
        serde_json::from_slice(content.as_bytes()).map_err(OrganizationError::InvalidManifest)?;
    if let Some(planned_files) = manifest.get_mut("planned_files") {
        match planned_files {
            serde_json::Value::Array(files) => files.retain(|file| {
                file.as_str().is_none_or(|file| {
                    !dropped_targets
                        .iter()
                        .any(|target| target == Path::new(file))
                })
            }),
            serde_json::Value::Object(files) => files.retain(|_, file| {
                file.as_str().is_none_or(|file| {
                    !dropped_targets
                        .iter()
                        .any(|target| target == Path::new(file))
                })
            }),
            _ => {}
        }
    }
    let mut bytes =
        serde_json::to_vec_pretty(&manifest).map_err(OrganizationError::SerializeManifest)?;
    bytes.push(b'\n');
    OutputOperation::write_bytes(target, PlannedContent::new(bytes)).map_err(Into::into)
}

fn absolute_path(path: &Path) -> Result<PathBuf, OrganizationError> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(OrganizationError::CurrentDirectory)
    }
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

fn safe_folder_name(value: &str, fallback: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned = cleaned.trim_matches([' ', '.']);
    if cleaned.is_empty() {
        fallback.to_owned()
    } else {
        cleaned.to_owned()
    }
}
