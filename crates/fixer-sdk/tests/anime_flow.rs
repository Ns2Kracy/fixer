use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Cour, LocalizedValue, WorkId,
};
use fixer_provider_bangumi::{BangumiConfig, BangumiProvider};
use fixer_provider_local::LocalProvider;
use fixer_sdk::Fixer;

fn local_anime() -> AnimeSeries {
    let mut titles = LocalizedValue::new();
    titles.insert("ja", "葬送のフリーレン".to_owned()).unwrap();
    titles.insert("zh-Hans", "葬送的芙莉莲".to_owned()).unwrap();
    titles.insert("zh-Hant", "葬送的芙莉蓮".to_owned()).unwrap();
    titles
        .insert("en", "Frieren: Beyond Journey's End".to_owned())
        .unwrap();
    let mut episode_titles = LocalizedValue::new();
    episode_titles
        .insert("ja", "冒険の終わり".to_owned())
        .unwrap();
    let episode = AnimeEpisode::new(
        WorkId::new("frieren-episode-1").unwrap(),
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

#[tokio::test]
async fn typed_anime_query_resolves_local_metadata_when_bangumi_is_offline() {
    let local = LocalProvider::from_anime_documents([local_anime()]).unwrap();
    let bangumi = BangumiProvider::new(BangumiConfig::default()).unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(bangumi)
        .preferred_languages(["zh-Hans", "ja", "en"])
        .unwrap()
        .offline()
        .build()
        .unwrap();

    let resolved = fixer
        .anime("葬送的芙莉莲")
        .year(2023)
        .resolve()
        .await
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "frieren");
    assert_eq!(resolved.value.titles.entries().len(), 4);
    assert_eq!(resolved.value.cours[0].episodes[0].aired_number, Some(1));
    assert_eq!(resolved.value.cours[0].episodes[0].absolute_number, Some(1));
    assert!(resolved.warnings.iter().any(|warning| {
        warning.code == "offline_provider_skipped" && warning.message.contains("network")
    }));
    assert_eq!(
        resolved.provenance.sources_for("anime.titles")[0]
            .provider
            .as_str(),
        "local"
    );
}
