use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Cour, FetchRequest,
    HttpClient, HttpError, HttpRequest, HttpResponse, LocalizedValue, MediaKind, Provider,
    SearchRequest, WorkId,
};
use fixer_provider_local::LocalProvider;

struct Offline;

impl HttpClient for Offline {
    fn execute<'a>(
        &'a self,
        _: HttpRequest,
    ) -> fixer_core::BoxFuture<'a, Result<HttpResponse, HttpError>> {
        panic!("local anime provider must not call HTTP")
    }
}

fn fixture_anime() -> AnimeSeries {
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

#[test]
fn pre_parsed_anime_searches_and_fetches_without_http() {
    let provider = LocalProvider::from_anime_documents([fixture_anime()]).unwrap();

    assert!(provider.descriptor().supports(MediaKind::Anime));
    assert!(!provider.descriptor().supports(MediaKind::Movie));
    assert!(!provider.descriptor().supports(MediaKind::Television));
    assert!(!provider.descriptor().requires_network());
    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::anime("葬送的芙莉莲", Some(2023)).unwrap(),
        &Offline,
    ))
    .unwrap();
    assert_eq!(candidates.len(), 1);
    let fixer_core::Candidate::Anime(candidate) = &candidates[0] else {
        panic!("expected anime candidate");
    };
    assert_eq!(candidate.title, "葬送のフリーレン");
    assert_eq!(candidate.year, Some(2023));
    assert_eq!(candidate.external_id.namespace, "local");

    let fetched = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Anime, candidates[0].external_id().clone()),
        &Offline,
    ))
    .unwrap();
    let fixer_core::MetadataDocument::Anime(anime) = fetched else {
        panic!("expected anime metadata");
    };
    assert_eq!(anime.titles.entries().len(), 4);
    assert_eq!(anime.cours[0].episodes[0].aired_number, Some(1));
    assert_eq!(anime.cours[0].episodes[0].absolute_number, Some(1));
}
