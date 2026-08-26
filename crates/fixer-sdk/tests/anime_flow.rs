use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Cour, LocalizedValue, WorkId,
};
use fixer_provider_anilist::{AniListConfig, AniListProvider};
use fixer_provider_bangumi::{BangumiConfig, BangumiProvider};
use fixer_provider_local::LocalProvider;
use fixer_sdk::Fixer;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method},
};

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

async fn anilist(server: &MockServer) -> AniListProvider {
    AniListProvider::new(
        AniListConfig::default()
            .with_endpoint(server.uri())
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn anilist_titles_and_artwork_merge_with_local_hierarchy() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(body_partial_json(serde_json::json!({
            "variables": { "search": "Frieren: Beyond Journey's End" }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-anilist/tests/fixtures/search.json"),
            "application/json",
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(body_partial_json(serde_json::json!({
            "variables": { "id": 154587 }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_str!("../../fixer-provider-anilist/tests/fixtures/fetch.json"),
            "application/json",
        ))
        .mount(&server)
        .await;

    let local = LocalProvider::from_anime_documents([local_anime()]).unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(anilist(&server).await)
        .preferred_languages(["ja", "ja-Latn", "zh-Hans", "zh-Hant", "en"])
        .unwrap()
        .build()
        .unwrap();

    let resolved = fixer
        .anime("Frieren: Beyond Journey's End")
        .year(2023)
        .resolve()
        .await
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "frieren");
    assert_eq!(resolved.value.cours.len(), 1);
    assert_eq!(resolved.value.cours[0].episodes[0].absolute_number, Some(1));
    assert_eq!(resolved.value.artwork.len(), 2);
    assert!(resolved.value.titles.entries().iter().any(|entry| {
        entry.value() == "Sousou no Frieren"
            && entry
                .language()
                .is_some_and(|language| language.as_str() == "ja-Latn")
    }));
    assert!(
        resolved.value.titles.entries().iter().any(|entry| {
            entry.value() == "Frieren at the Funeral" && entry.language().is_none()
        })
    );
    assert!(
        resolved
            .provenance
            .sources_for("anime.titles")
            .iter()
            .any(|source| source.provider.as_str() == "local")
    );
    assert!(
        resolved
            .provenance
            .sources_for("anime.titles")
            .iter()
            .any(|source| source.provider.as_str() == "anilist")
    );
    assert!(
        resolved
            .provenance
            .sources_for("anime.artwork")
            .iter()
            .all(|source| source.provider.as_str() == "anilist")
    );
    assert_eq!(
        resolved.provenance.sources_for("anime.cours")[0]
            .provider
            .as_str(),
        "local"
    );
}

#[tokio::test]
async fn anilist_failure_is_a_warning_when_local_anime_is_sufficient() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let local = LocalProvider::from_anime_documents([local_anime()]).unwrap();
    let fixer = Fixer::builder()
        .provider(local)
        .provider(anilist(&server).await)
        .build()
        .unwrap();

    let resolved = fixer
        .anime("Frieren: Beyond Journey's End")
        .year(2023)
        .resolve()
        .await
        .unwrap();

    assert_eq!(resolved.value.id.as_str(), "frieren");
    assert_eq!(resolved.value.cours.len(), 1);
    assert!(resolved.warnings.iter().any(|warning| {
        warning.code == "provider_search_failed" && warning.message.contains("AniList")
    }));
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
