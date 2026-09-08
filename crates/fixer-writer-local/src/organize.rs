//! Destination layout and placement planning shared by CLI and server jobs.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
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
    pub const fn new(
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
    #[error("multiple media files identify episode S{season:02}E{episode:02}")]
    AmbiguousEpisode { season: u32, episode: u32 },
    #[error("multiple source files map to organization target `{0}`")]
    DuplicateTarget(PathBuf),
    #[error("relative symlink cannot be represented from `{from}` to `{to}`")]
    RelativeSymlinkUnavailable { from: PathBuf, to: PathBuf },
    #[error("failed to determine the current directory: {0}")]
    CurrentDirectory(std::io::Error),
    #[error("invalid planned manifest: {0}")]
    InvalidManifest(serde_json::Error),
    #[error("could not serialize planned manifest: {0}")]
    SerializeManifest(serde_json::Error),
    #[error("could not inspect organization source `{path}`: {source}")]
    InspectSource {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Builds a single bounded plan whose targets are relative to `destination_path`.
pub fn organize(request: OrganizationRequest<'_>) -> Result<OutputPlan, OrganizationError> {
    let OrganizationRequest {
        source_path,
        destination_path,
        media,
        placement,
        path_template,
        media_target: target_override,
        metadata_plan,
    } = request;
    let package = package_path(media, path_template)?;
    let television = matches!(media, OrganizationMedia::Television(_));
    let directory = source_path.is_dir();
    let files = if directory {
        source_files(source_path)?
    } else {
        vec![source_path.to_path_buf()]
    };
    let primary = sole_primary_file(&files, media);
    let mut episode_targets = BTreeMap::new();
    let mut placements = Vec::new();
    for source in &files {
        let relative = if directory {
            source
                .strip_prefix(source_path)
                .map_err(|_| OrganizationError::MissingFileName(source.clone()))?
        } else {
            Path::new(
                source
                    .file_name()
                    .ok_or_else(|| OrganizationError::MissingFileName(source.clone()))?,
            )
        };
        let episode = television
            .then(|| crate::episode_path::episode_number(source))
            .flatten()
            .filter(|_| is_primary_media_file(source, media));
        let target = if Some(source.as_path()) == primary || !directory {
            target_override.map_or_else(
                || media_target(source, media, &package),
                |target| Ok(package.join(target)),
            )?
        } else if episode.is_some() {
            media_target(source, media, &package)?
        } else {
            package.join(relative)
        };
        if let Some((season, episode)) = episode {
            if episode_targets
                .insert((season, episode), target.with_extension("nfo"))
                .is_some()
            {
                return Err(OrganizationError::AmbiguousEpisode { season, episode });
            }
        }
        placements.push((source, target));
    }
    if television {
        colocate_episode_sidecars(&mut placements, &episode_targets, media);
    }
    let metadata_operations =
        organize_metadata_operations(&metadata_plan, &package, &episode_targets, television)?;
    let reserved_targets = metadata_operations
        .iter()
        .filter_map(OutputOperation::target)
        .map(Path::to_path_buf)
        .collect::<BTreeSet<_>>();
    let mut placed_targets = BTreeSet::new();
    let mut plan = OutputPlan::new(destination_path);
    for (source, target) in placements {
        if !reserved_targets.contains(&target) {
            if !placed_targets.insert(target.clone()) {
                return Err(OrganizationError::DuplicateTarget(target));
            }
            plan.push(placement_operation(
                source,
                destination_path,
                target,
                placement,
            )?);
        }
    }
    for operation in metadata_operations {
        plan.push(operation);
    }
    Ok(plan)
}

fn organize_metadata_operations(
    metadata_plan: &OutputPlan,
    package: &Path,
    episode_targets: &BTreeMap<(u32, u32), PathBuf>,
    television: bool,
) -> Result<Vec<OutputOperation>, CoreError> {
    metadata_plan
        .operations()
        .iter()
        .map(|operation| {
            if television {
                let episode_target = operation
                    .target()
                    .filter(|path| {
                        path.extension().is_some_and(|extension| extension == "nfo")
                            && path
                                .file_stem()
                                .and_then(|stem| stem.to_str())
                                .is_some_and(|stem| stem.starts_with('S'))
                    })
                    .and_then(crate::episode_path::episode_number)
                    .and_then(|episode| episode_targets.get(&episode));
                if let (Some(target), OutputOperation::WriteBytes { content, .. }) =
                    (episode_target, operation)
                {
                    return OutputOperation::write_bytes(target, content.clone());
                }
            }
            rebase_operation(operation, package)
        })
        .collect()
}

fn colocate_episode_sidecars(
    placements: &mut [(&PathBuf, PathBuf)],
    episode_targets: &BTreeMap<(u32, u32), PathBuf>,
    media: OrganizationMedia<'_>,
) {
    let primary = placements
        .iter()
        .filter(|(source, _)| {
            is_primary_media_file(source, media)
                && crate::episode_path::episode_number(source).is_some()
        })
        .map(|(source, target)| ((*source).clone(), target.clone()))
        .collect::<Vec<_>>();
    for (source, target) in placements {
        let extension = source
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !["srt", "ass", "ssa", "sub", "idx", "vtt", "sup", "nfo"].contains(&extension.as_str()) {
            continue;
        }
        if extension == "nfo" {
            if let Some(nfo) = crate::episode_path::episode_number(source)
                .and_then(|key| episode_targets.get(&key))
            {
                target.clone_from(nfo);
                continue;
            }
        }
        let Some(name) = source.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let matches = primary
            .iter()
            .filter(|(video, _)| {
                video
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| {
                        name.strip_prefix(stem)
                            .is_some_and(|suffix| suffix.starts_with('.'))
                    })
            })
            .collect::<Vec<_>>();
        if let [(video, destination)] = matches.as_slice() {
            if let (Some(parent), Some(stem), Some(video_stem)) = (
                destination.parent(),
                destination.file_stem().and_then(|stem| stem.to_str()),
                video.file_stem().and_then(|stem| stem.to_str()),
            ) {
                *target = parent.join(format!("{stem}{}", &name[video_stem.len()..]));
            }
        }
    }
}

fn source_files(root: &Path) -> Result<Vec<PathBuf>, OrganizationError> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)
            .map_err(|source| OrganizationError::InspectSource {
                path: directory.clone(),
                source,
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|source| OrganizationError::InspectSource {
                path: directory.clone(),
                source,
            })?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            let metadata =
                fs::symlink_metadata(&path).map_err(|source| OrganizationError::InspectSource {
                    path: path.clone(),
                    source,
                })?;
            if metadata.is_dir() {
                pending.push(path);
            } else if metadata.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn sole_primary_file<'a>(files: &'a [PathBuf], media: OrganizationMedia<'_>) -> Option<&'a Path> {
    let mut primary = files
        .iter()
        .filter(|path| is_primary_media_file(path, media));
    let first = primary.next()?;
    primary.next().is_none().then_some(first.as_path())
}

fn is_primary_media_file(path: &Path, media: OrganizationMedia<'_>) -> bool {
    let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
        return false;
    };
    let extension = extension.to_ascii_lowercase();
    let supported = match media {
        OrganizationMedia::Movie(_)
        | OrganizationMedia::Television(_)
        | OrganizationMedia::Anime(_) => {
            &["avi", "m2ts", "m4v", "mkv", "mov", "mp4", "ts", "webm"][..]
        }
        OrganizationMedia::Music(_) => &["aac", "flac", "m4a", "mp3", "ogg", "opus", "wav"][..],
        OrganizationMedia::Book(_) => &["azw3", "cbr", "cbz", "epub", "mobi", "pdf"][..],
    };
    supported.contains(&extension.as_str())
}

/// Removes writer-declared asset transfers while keeping manifests truthful.
pub fn metadata_only(plan: &OutputPlan) -> Result<OutputPlan, OrganizationError> {
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
            OutputOperation::WriteBytes { target, content }
                if target.file_name().and_then(|name| name.to_str())
                    == Some("fixer-manifest.json") =>
            {
                filtered.push(reconcile_manifest(target, content, &dropped_targets)?);
            }
            OutputOperation::CreateDirectory { .. } | OutputOperation::WriteBytes { .. } => {
                filtered.push(operation.clone());
            }
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
        OrganizationMedia::Television(_) => crate::episode_path::episode_number(source)
            .map_or_else(
                || package.join(file_name),
                |(season, _)| package.join(format!("Season {season:02}")).join(file_name),
            ),
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
