//! Deterministic Kodi/Jellyfin-compatible television NFO planning.

use crate::nfo::{element, escape_xml};
use fixer_core::{
    ArtworkKind, Episode, MetadataDocument, OrderingScheme, OutputOperation, OutputPlan,
    PlannedContent, PlanningError, Resolved, Season, Series, WriteRequest, Writer,
};
use std::path::{Path, PathBuf};

/// Built-in deterministic television hierarchy NFO writer.
#[derive(Debug, Clone, Default)]
pub struct TelevisionWriter;

impl TelevisionWriter {
    /// Plans series, season, and episode NFO files without touching the filesystem.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<Series>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        plan_series(&resolved.value, output_root.as_ref())
    }
}

impl Writer for TelevisionWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Television(series) => plan_series(&series, &request.output_root),
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}

pub(crate) fn plan_series(
    series: &Series,
    output_root: &Path,
) -> Result<OutputPlan, PlanningError> {
    let mut plan = OutputPlan::new(output_root);
    plan.push(OutputOperation::write_bytes(
        "tvshow.nfo",
        PlannedContent::new(series_xml(series).into_bytes()),
    )?);
    for season in &series.seasons {
        let directory = season_directory(season.number);
        plan.push(OutputOperation::write_bytes(
            directory.join("season.nfo"),
            PlannedContent::new(season_xml(season).into_bytes()),
        )?);
        for episode in &season.episodes {
            plan.push(OutputOperation::write_bytes(
                directory.join(format!("{}.nfo", episode_stem(episode))),
                PlannedContent::new(episode_xml(episode).into_bytes()),
            )?);
        }
    }
    Ok(plan)
}

fn series_xml(series: &Series) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tvshow>\n");
    element(&mut xml, "title", first_title(&series.titles));
    if let Some(summary) = first_summary(&series.summaries) {
        element(&mut xml, "plot", summary);
    }
    element(
        &mut xml,
        "displayorder",
        match series.ordering {
            OrderingScheme::Aired => "aired",
            OrderingScheme::Dvd => "dvd",
            OrderingScheme::Absolute => "absolute",
        },
    );
    artwork_elements(&mut xml, &series.artwork);
    xml.push_str("</tvshow>\n");
    xml
}

fn season_xml(season: &Season) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<season>\n");
    let title = if season.number == 0 {
        "Specials".to_owned()
    } else {
        format!("Season {}", season.number)
    };
    element(&mut xml, "title", &title);
    element(&mut xml, "seasonnumber", &season.number.to_string());
    artwork_elements(&mut xml, &season.artwork);
    xml.push_str("</season>\n");
    xml
}

fn episode_xml(episode: &Episode) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<episodedetails>\n");
    element(&mut xml, "title", first_title(&episode.titles));
    if let Some(summary) = first_summary(&episode.summaries) {
        element(&mut xml, "plot", summary);
    }
    if let Some(season) = episode.sequence.season {
        element(&mut xml, "season", &season.to_string());
    }
    element(&mut xml, "episode", &episode.sequence.episode.to_string());
    if let Some(runtime) = episode.runtime {
        element(
            &mut xml,
            "runtime",
            &runtime.as_seconds().div_ceil(60).to_string(),
        );
    }
    artwork_elements(&mut xml, &episode.artwork);
    xml.push_str("</episodedetails>\n");
    xml
}

fn artwork_elements(output: &mut String, artwork: &[fixer_core::ArtworkReference]) {
    for item in artwork {
        let aspect = match item.kind {
            ArtworkKind::Poster | ArtworkKind::Cover => "poster",
            ArtworkKind::Backdrop => "fanart",
            ArtworkKind::Banner => "banner",
            ArtworkKind::Logo => "clearlogo",
            ArtworkKind::Profile => "profile",
        };
        output.push_str("  <thumb aspect=\"");
        output.push_str(aspect);
        output.push_str("\">");
        output.push_str(&escape_xml(&item.location));
        output.push_str("</thumb>\n");
    }
}

fn first_title(titles: &fixer_core::LocalizedValue<String>) -> &str {
    titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .unwrap_or_default()
}

fn first_summary(summaries: &fixer_core::LocalizedValue<String>) -> Option<&str> {
    summaries
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
}

fn season_directory(number: u32) -> PathBuf {
    PathBuf::from(format!("Season {number:02}"))
}

fn episode_stem(episode: &Episode) -> String {
    match episode.sequence.scheme {
        OrderingScheme::Aired | OrderingScheme::Dvd => format!(
            "S{:02}E{:02}",
            episode.sequence.season.unwrap_or_default(),
            episode.sequence.episode
        ),
        OrderingScheme::Absolute => format!("E{:04}", episode.sequence.episode),
    }
}
