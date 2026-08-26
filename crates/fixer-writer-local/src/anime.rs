//! Deterministic anime hierarchy and NFO planning.

use crate::nfo::element;
use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, MetadataDocument, OutputOperation, OutputPlan,
    PlannedContent, PlanningError, Resolved, WriteRequest, Writer,
};
use std::path::{Path, PathBuf};

/// Built-in deterministic anime hierarchy writer.
#[derive(Debug, Clone, Default)]
pub struct AnimeWriter;

impl AnimeWriter {
    /// Plans anime, cour, and episode NFO files without touching the filesystem.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<AnimeSeries>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        plan_anime(&resolved.value, output_root.as_ref())
    }
}

impl Writer for AnimeWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Anime(anime) => plan_anime(&anime, &request.output_root),
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}

fn plan_anime(anime: &AnimeSeries, output_root: &Path) -> Result<OutputPlan, PlanningError> {
    let mut plan = OutputPlan::new(output_root);
    plan.push(OutputOperation::write_bytes(
        "anime.nfo",
        PlannedContent::new(anime_xml(anime).into_bytes()),
    )?);
    for cour in &anime.cours {
        let directory = cour_directory(cour.number);
        plan.push(OutputOperation::write_bytes(
            directory.join("cour.nfo"),
            PlannedContent::new(cour_xml(cour.number).into_bytes()),
        )?);
        for episode in &cour.episodes {
            plan.push(OutputOperation::write_bytes(
                directory.join(format!("{}.nfo", episode_stem(cour.number, episode)?)),
                PlannedContent::new(episode_xml(cour.number, episode).into_bytes()),
            )?);
        }
    }
    Ok(plan)
}

fn anime_xml(anime: &AnimeSeries) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<anime>\n");
    element(&mut xml, "title", first_title(&anime.titles));
    if let Some(summary) = first_summary(&anime.summaries) {
        element(&mut xml, "plot", summary);
    }
    element(&mut xml, "relation", relation_name(anime.relation));
    xml.push_str("</anime>\n");
    xml
}

fn cour_xml(number: u32) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<courdetails>\n");
    element(&mut xml, "title", &format!("Cour {number}"));
    element(&mut xml, "cour", &number.to_string());
    xml.push_str("</courdetails>\n");
    xml
}

fn episode_xml(cour: u32, episode: &AnimeEpisode) -> String {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<episodedetails>\n");
    element(&mut xml, "title", first_title(&episode.titles));
    element(&mut xml, "cour", &cour.to_string());
    element(&mut xml, "episodeclass", class_name(episode.class));
    if let Some(number) = episode.aired_number {
        element(&mut xml, "airednumber", &number.to_string());
    }
    if let Some(number) = episode.absolute_number {
        element(&mut xml, "absolutenumber", &number.to_string());
    }
    xml.push_str("</episodedetails>\n");
    xml
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

fn cour_directory(number: u32) -> PathBuf {
    PathBuf::from(format!("Cour {number:02}"))
}

fn episode_stem(cour: u32, episode: &AnimeEpisode) -> Result<String, PlanningError> {
    let class = match episode.class {
        AnimeEpisodeClass::Regular => "E",
        AnimeEpisodeClass::Ova => "OVA",
        AnimeEpisodeClass::Ona => "ONA",
        AnimeEpisodeClass::Special => "SP",
    };
    match (episode.aired_number, episode.absolute_number) {
        (Some(number), _) => Ok(format!("C{cour:02}{class}{number:03}")),
        (None, Some(number)) => Ok(format!("C{cour:02}{class}A{number:04}")),
        (None, None) => Err(PlanningError::InvalidPlan(format!(
            "anime episode `{}` has no aired or absolute number",
            episode.id.as_str()
        ))),
    }
}

const fn class_name(class: AnimeEpisodeClass) -> &'static str {
    match class {
        AnimeEpisodeClass::Regular => "regular",
        AnimeEpisodeClass::Ova => "ova",
        AnimeEpisodeClass::Ona => "ona",
        AnimeEpisodeClass::Special => "special",
    }
}

const fn relation_name(relation: fixer_core::AnimeSeriesRelation) -> &'static str {
    match relation {
        fixer_core::AnimeSeriesRelation::Original => "original",
        fixer_core::AnimeSeriesRelation::Adaptation => "adaptation",
        fixer_core::AnimeSeriesRelation::Sequel => "sequel",
        fixer_core::AnimeSeriesRelation::Prequel => "prequel",
        fixer_core::AnimeSeriesRelation::SideStory => "side_story",
        fixer_core::AnimeSeriesRelation::SpinOff => "spin_off",
    }
}
