use fixer_core::{AnimeEpisodeClass, AnimeSeriesRelation};
use fixer_provider_local::scan_anime;

#[test]
fn scans_writer_hierarchy_without_flattening_episode_identity() {
    let root = tempfile::tempdir().unwrap();
    let anime = root.path().join("Frieren");
    let first_cour = anime.join("Cour 01");
    let second_cour = anime.join("Cour 02");
    std::fs::create_dir_all(&first_cour).unwrap();
    std::fs::create_dir_all(&second_cour).unwrap();
    std::fs::write(
        anime.join("anime.nfo"),
        r#"<anime>
            <title>葬送のフリーレン</title>
            <plot>旅の終わりから始まる物語。</plot>
            <relation>adaptation</relation>
        </anime>"#,
    )
    .unwrap();
    std::fs::write(
        first_cour.join("cour.nfo"),
        "<courdetails><title>Cour 1</title><cour>1</cour></courdetails>",
    )
    .unwrap();
    std::fs::write(
        first_cour.join("C01E001.nfo"),
        r#"<episodedetails>
            <title>冒険の終わり</title>
            <cour>1</cour>
            <episodeclass>regular</episodeclass>
            <airednumber>1</airednumber>
            <absolutenumber>1</absolutenumber>
        </episodedetails>"#,
    )
    .unwrap();
    std::fs::write(
        first_cour.join("C01OVA001.nfo"),
        r#"<episodedetails>
            <title>特別編</title>
            <cour>1</cour>
            <episodeclass>ova</episodeclass>
            <airednumber>1</airednumber>
            <absolutenumber>29</absolutenumber>
        </episodedetails>"#,
    )
    .unwrap();
    std::fs::write(
        second_cour.join("cour.nfo"),
        "<courdetails><title>Cour 2</title><cour>2</cour></courdetails>",
    )
    .unwrap();
    std::fs::write(
        second_cour.join("C02ONA001.nfo"),
        r#"<episodedetails>
            <title>配信編</title>
            <cour>2</cour>
            <episodeclass>ona</episodeclass>
            <airednumber>1</airednumber>
            <absolutenumber>30</absolutenumber>
        </episodedetails>"#,
    )
    .unwrap();
    std::fs::write(
        second_cour.join("C02SPA0042.nfo"),
        r#"<episodedetails>
            <title>総集編</title>
            <cour>2</cour>
            <episodeclass>special</episodeclass>
            <absolutenumber>42</absolutenumber>
        </episodedetails>"#,
    )
    .unwrap();

    let result = scan_anime(root.path()).unwrap();
    assert!(result.warnings.is_empty());
    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.roots, vec![anime]);
    let series = &result.documents[0];
    assert_eq!(series.relation, AnimeSeriesRelation::Adaptation);
    assert_eq!(series.cours.len(), 2);
    assert_eq!(
        series.cours[0].episodes[0].class,
        AnimeEpisodeClass::Regular
    );
    assert_eq!(series.cours[0].episodes[1].class, AnimeEpisodeClass::Ova);
    assert_eq!(series.cours[1].episodes[0].class, AnimeEpisodeClass::Ona);
    assert_eq!(
        series.cours[1].episodes[1].class,
        AnimeEpisodeClass::Special
    );
    assert_eq!(series.cours[1].episodes[1].aired_number, None);
    assert_eq!(series.cours[1].episodes[1].absolute_number, Some(42));
}

struct Offline;

impl fixer_core::HttpClient for Offline {
    fn execute<'a>(
        &'a self,
        _: fixer_core::HttpRequest,
    ) -> fixer_core::BoxFuture<'a, Result<fixer_core::HttpResponse, fixer_core::HttpError>> {
        panic!("scanned anime provider must not call HTTP")
    }
}

#[test]
fn scanned_anime_is_registered_as_an_offline_provider_document() {
    use fixer_core::{FetchRequest, MediaKind, Provider, SearchRequest};
    use fixer_provider_local::LocalProvider;

    let root = tempfile::tempdir().unwrap();
    let anime = root.path().join("Example");
    let cour = anime.join("Cour 01");
    std::fs::create_dir_all(&cour).unwrap();
    std::fs::write(
        anime.join("anime.nfo"),
        "<anime><title>Example Anime</title><relation>original</relation></anime>",
    )
    .unwrap();
    std::fs::write(
        cour.join("cour.nfo"),
        "<courdetails><cour>1</cour></courdetails>",
    )
    .unwrap();
    std::fs::write(
        cour.join("C01E001.nfo"),
        "<episodedetails><title>Episode 1</title><cour>1</cour><episodeclass>regular</episodeclass><airednumber>1</airednumber><absolutenumber>1</absolutenumber></episodedetails>",
    )
    .unwrap();

    let (provider, warnings) = LocalProvider::from_scan(root.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(provider.descriptor().supports(MediaKind::Anime));
    assert!(!provider.descriptor().supports(MediaKind::Movie));
    assert!(!provider.descriptor().requires_network());

    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::anime("Example Anime", None).unwrap(),
        &Offline,
    ))
    .unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].external_id().namespace, "local");
    let document = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Anime, candidates[0].external_id().clone()),
        &Offline,
    ))
    .unwrap();
    assert_eq!(document.media_kind(), MediaKind::Anime);
}

#[test]
fn malformed_episode_sidecars_become_path_warnings() {
    let root = tempfile::tempdir().unwrap();
    let anime = root.path().join("Example");
    let cour = anime.join("Cour 01");
    std::fs::create_dir_all(&cour).unwrap();
    std::fs::write(
        anime.join("anime.nfo"),
        "<anime><title>Example</title><relation>original</relation></anime>",
    )
    .unwrap();
    std::fs::write(
        cour.join("cour.nfo"),
        "<courdetails><cour>1</cour></courdetails>",
    )
    .unwrap();
    let broken = cour.join("broken.nfo");
    std::fs::write(&broken, "<episodedetails><title>Broken").unwrap();

    let result = scan_anime(root.path()).unwrap();
    assert_eq!(result.documents.len(), 1);
    assert!(result.warnings.iter().any(|warning| warning.path == broken));
}
