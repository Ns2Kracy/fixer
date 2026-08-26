use fixer_core::{
    AnimeDocument, AnimeEpisode, AnimeEpisodeClass, AnimeMerger, AnimeSeries, AnimeSeriesRelation,
    ArtworkKind, ArtworkReference, Cour, ExternalId, LocalizedValue, MergePolicy, ProviderId,
    SourceRef, WorkId,
};
use std::time::SystemTime;

fn source(provider: &str, id: &str) -> SourceRef {
    SourceRef::new(
        ProviderId::new(provider).unwrap(),
        Some(ExternalId::new(provider, id).unwrap()),
        None,
        SystemTime::UNIX_EPOCH,
    )
}

fn local_anime() -> AnimeSeries {
    let mut titles = LocalizedValue::new();
    titles.insert("ja", "葬送のフリーレン".to_owned()).unwrap();
    titles.insert("zh-Hans", "葬送的芙莉莲".to_owned()).unwrap();
    let mut episode_titles = LocalizedValue::new();
    episode_titles
        .insert("ja", "冒険の終わり".to_owned())
        .unwrap();
    let episode = AnimeEpisode::new(
        WorkId::new("episode-1").unwrap(),
        episode_titles,
        AnimeEpisodeClass::Regular,
        Some(1),
        Some(1),
    )
    .unwrap();
    AnimeSeries::new(
        WorkId::new("frieren").unwrap(),
        titles,
        AnimeSeriesRelation::Adaptation,
        vec![Cour::new(1, vec![episode]).unwrap()],
    )
}

fn anilist_anime() -> AnimeSeries {
    let mut titles = LocalizedValue::new();
    titles
        .insert("ja-Latn", "Sousou no Frieren".to_owned())
        .unwrap();
    titles
        .insert("en", "Frieren: Beyond Journey's End".to_owned())
        .unwrap();
    titles.insert_untagged("Frieren at the Funeral".to_owned());
    let mut anime = AnimeSeries::new(
        WorkId::new("anilist-154587").unwrap(),
        titles,
        AnimeSeriesRelation::Original,
        Vec::new(),
    );
    anime.artwork.push(
        ArtworkReference::new(ArtworkKind::Cover, "https://images.example/cover.jpg")
            .unwrap()
            .with_external_id(ExternalId::new("anilist-artwork", "154587-cover").unwrap()),
    );
    anime.artwork.push(
        ArtworkReference::new(ArtworkKind::Banner, "https://images.example/banner.jpg").unwrap(),
    );
    anime
}

#[test]
fn complementary_titles_and_artwork_merge_without_replacing_hierarchy() {
    let merger = AnimeMerger::new(MergePolicy::new([
        ProviderId::new("local").unwrap(),
        ProviderId::new("anilist").unwrap(),
    ]));
    let resolved = merger
        .merge([
            AnimeDocument::new(local_anime(), source("local", "frieren")),
            AnimeDocument::new(anilist_anime(), source("anilist", "154587")),
        ])
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "frieren");
    assert_eq!(resolved.value.relation, AnimeSeriesRelation::Adaptation);
    assert_eq!(resolved.value.cours.len(), 1);
    assert_eq!(resolved.value.cours[0].episodes[0].aired_number, Some(1));
    assert_eq!(resolved.value.cours[0].episodes[0].absolute_number, Some(1));
    let title_values = resolved
        .value
        .titles
        .entries()
        .iter()
        .map(|entry| entry.value().as_str())
        .collect::<Vec<_>>();
    for expected in [
        "葬送のフリーレン",
        "葬送的芙莉莲",
        "Sousou no Frieren",
        "Frieren: Beyond Journey's End",
        "Frieren at the Funeral",
    ] {
        assert!(title_values.contains(&expected));
    }
    assert_eq!(resolved.value.artwork.len(), 2);
    assert_eq!(
        resolved
            .provenance
            .sources_for("anime.titles")
            .iter()
            .map(|source| source.provider.as_str())
            .collect::<Vec<_>>(),
        vec!["local", "local", "anilist", "anilist", "anilist"]
    );
    assert_eq!(
        resolved.provenance.sources_for("anime.artwork")[0]
            .provider
            .as_str(),
        "anilist"
    );
    assert_eq!(
        resolved.provenance.sources_for("anime.cours")[0]
            .provider
            .as_str(),
        "local"
    );
    assert_eq!(resolved.completeness, 1.0);
}

#[test]
fn merger_uses_the_first_nonempty_hierarchy_by_precedence() {
    let merger = AnimeMerger::new(MergePolicy::new([
        ProviderId::new("anilist").unwrap(),
        ProviderId::new("local").unwrap(),
    ]));
    let resolved = merger
        .merge([
            AnimeDocument::new(anilist_anime(), source("anilist", "154587")),
            AnimeDocument::new(local_anime(), source("local", "frieren")),
        ])
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "frieren");
    assert_eq!(resolved.value.cours.len(), 1);
    assert_eq!(resolved.value.artwork.len(), 2);
}
