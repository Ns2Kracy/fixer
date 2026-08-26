//! Deterministic music metadata and tag-update intent planning.

use fixer_core::{
    MetadataDocument, MusicReleaseGroup, OutputOperation, OutputPlan, PlannedContent,
    PlanningError, ProvenanceMap, Resolved, Track, WriteRequest, Writer,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

const HARDLINK_WARNING: &str = "In-place tag mutation changes every path hardlinked to the same audio file; confirm only after reviewing all hardlinked paths.";

/// Plans music JSON artifacts and explicit tag-update declarations without mutating audio.
#[derive(Debug, Clone, Default)]
pub struct MusicWriter {
    tag_targets: BTreeMap<String, PathBuf>,
}

impl MusicWriter {
    /// Constructs a writer with explicit track-identity to audio-path tag targets.
    ///
    /// Targets are declarations only. The resulting plan writes a confirmation-gated
    /// intent document and never writes, copies, or links an audio path.
    pub const fn with_tag_targets(tag_targets: BTreeMap<String, PathBuf>) -> Self {
        Self { tag_targets }
    }

    /// Plans album JSON, a manifest, and optional tag-update intent declarations.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<MusicReleaseGroup>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        self.plan_group(
            &resolved.value,
            Some(&resolved.provenance),
            output_root.as_ref(),
        )
    }

    fn plan_group(
        &self,
        group: &MusicReleaseGroup,
        provenance: Option<&ProvenanceMap>,
        output_root: &Path,
    ) -> Result<OutputPlan, PlanningError> {
        let tracks = tracks_by_identity(group)?;
        for identity in self.tag_targets.keys() {
            if !tracks.contains_key(identity) {
                return Err(PlanningError::InvalidPlan(format!(
                    "tag target references unknown track identity `{identity}`"
                )));
            }
        }

        let mut plan = OutputPlan::new(output_root);
        plan.push(write_json("album.json", group)?);

        let has_tag_intent = !self.tag_targets.is_empty();
        if has_tag_intent {
            let updates = self
                .tag_targets
                .iter()
                .map(|(identity, path)| {
                    let track = tracks[identity];
                    TagUpdate {
                        track_identity: identity,
                        audio_path: path,
                        artist: &group.artist.name,
                        album: first_title(&group.titles),
                        title: first_title(&track.titles),
                        disc: track.sequence.disc,
                        track: track.sequence.track,
                    }
                })
                .collect::<Vec<_>>();
            let intent = TagUpdateIntent {
                schema_version: 1,
                requires_confirmation: true,
                warning: HARDLINK_WARNING,
                updates,
            };
            plan.push(write_json("tag-update-intent.json", &intent)?);
        }

        let mut planned_files = BTreeSet::from(["album.json"]);
        if has_tag_intent {
            planned_files.insert("tag-update-intent.json");
        }
        let manifest = MusicManifest {
            schema_version: 1,
            work_id: group.id.as_str(),
            provenance,
            planned_files,
        };
        plan.push(write_json("fixer-manifest.json", &manifest)?);
        Ok(plan)
    }
}

impl Writer for MusicWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Music(group) => {
                self.plan_group(&group, None, request.output_root.as_path())
            }
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}

#[derive(Serialize)]
struct TagUpdateIntent<'a> {
    schema_version: u8,
    requires_confirmation: bool,
    warning: &'static str,
    updates: Vec<TagUpdate<'a>>,
}

#[derive(Serialize)]
struct TagUpdate<'a> {
    track_identity: &'a str,
    audio_path: &'a Path,
    artist: &'a str,
    album: &'a str,
    title: &'a str,
    disc: u32,
    track: u32,
}

#[derive(Serialize)]
struct MusicManifest<'a> {
    schema_version: u8,
    work_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<&'a ProvenanceMap>,
    planned_files: BTreeSet<&'static str>,
}

fn tracks_by_identity(
    group: &MusicReleaseGroup,
) -> Result<BTreeMap<String, &Track>, PlanningError> {
    let mut tracks = BTreeMap::new();
    for track in group
        .releases
        .iter()
        .flat_map(|release| &release.discs)
        .flat_map(|disc| &disc.tracks)
    {
        if tracks.insert(track.id.as_str().to_owned(), track).is_some() {
            return Err(PlanningError::InvalidPlan(format!(
                "duplicate track identity `{}`",
                track.id.as_str()
            )));
        }
    }
    Ok(tracks)
}

fn first_title(titles: &fixer_core::LocalizedValue<String>) -> &str {
    titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .unwrap_or_default()
}

fn write_json(
    target: &'static str,
    value: &impl Serialize,
) -> Result<OutputOperation, PlanningError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(OutputOperation::write_bytes(
        target,
        PlannedContent::new(bytes),
    )?)
}
