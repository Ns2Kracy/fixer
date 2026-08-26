use fixer_core::{
    ArtworkKind, ArtworkReference, Episode, EpisodeSequence, LocalizedValue, OrderingScheme,
    OutputOperation, ProvenanceMap, Resolved, Season, Series, WorkId,
};
use fixer_writer_local::TelevisionWriter;
use std::path::PathBuf;

fn resolved_series() -> Resolved<Series> {
    let mut episode_titles = LocalizedValue::new();
    episode_titles
        .insert("en", "The Kingsroad".to_owned())
        .unwrap();
    let mut episode = Episode::new(
        WorkId::new("episode-1-2").unwrap(),
        episode_titles,
        EpisodeSequence::aired(1, 2).unwrap(),
    );
    episode
        .summaries
        .insert("en", "The royal family arrives.".to_owned())
        .unwrap();
    episode.artwork.push(
        ArtworkReference::new(ArtworkKind::Backdrop, "https://images/episode-still.jpg").unwrap(),
    );

    let mut season = Season::new(WorkId::new("season-1").unwrap(), 1, vec![episode]).unwrap();
    season.artwork.push(
        ArtworkReference::new(ArtworkKind::Poster, "https://images/season-poster.jpg").unwrap(),
    );

    let mut series_titles = LocalizedValue::new();
    series_titles
        .insert("en", "Example Show".to_owned())
        .unwrap();
    let mut series = Series::new(
        WorkId::new("series-1").unwrap(),
        series_titles,
        OrderingScheme::Aired,
        vec![season],
    );
    series
        .summaries
        .insert("en", "A series summary.".to_owned())
        .unwrap();
    series.artwork.push(
        ArtworkReference::new(ArtworkKind::Poster, "https://images/series-poster.jpg").unwrap(),
    );

    Resolved {
        value: series,
        provenance: ProvenanceMap::new(),
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings: Vec::new(),
    }
}

#[test]
fn plans_series_season_episode_nfos_with_artwork_paths() {
    let plan = TelevisionWriter
        .plan_resolved(&resolved_series(), "library")
        .unwrap();
    let targets = plan
        .operations()
        .iter()
        .filter_map(OutputOperation::target)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        vec![
            PathBuf::from("tvshow.nfo"),
            PathBuf::from("Season 01/season.nfo"),
            PathBuf::from("Season 01/S01E02.nfo"),
        ]
    );
    let contents = plan
        .operations()
        .iter()
        .filter_map(|operation| match operation {
            OutputOperation::WriteBytes { content, .. } => {
                Some(String::from_utf8(content.as_bytes().to_vec()).unwrap())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(contents[0].contains("https://images/series-poster.jpg"));
    assert!(contents[1].contains("https://images/season-poster.jpg"));
    assert!(contents[2].contains("https://images/episode-still.jpg"));
    assert!(contents[2].contains("<season>1</season>"));
    assert!(contents[2].contains("<episode>2</episode>"));
}

#[test]
fn absolute_ordering_uses_absolute_episode_paths() {
    let mut resolved = resolved_series();
    resolved.value.ordering = OrderingScheme::Absolute;
    resolved.value.seasons[0].episodes[0].sequence = EpisodeSequence::absolute(2).unwrap();
    let plan = TelevisionWriter
        .plan_resolved(&resolved, "library")
        .unwrap();
    assert_eq!(
        plan.operations()[2].target().unwrap(),
        std::path::Path::new("Season 01/E0002.nfo")
    );
}
