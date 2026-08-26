use fixer_core::{
    AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Cour, LocalizedValue,
    MetadataDocument, OutputOperation, ProvenanceMap, Resolved, WorkId, WriteRequest, Writer,
};
use fixer_writer_local::AnimeWriter;
use std::path::{Path, PathBuf};

fn titles(locale: &str, value: &str) -> LocalizedValue<String> {
    let mut titles = LocalizedValue::new();
    titles.insert(locale, value.to_owned()).unwrap();
    titles
}

fn episode(
    id: &str,
    title: &str,
    class: AnimeEpisodeClass,
    aired: Option<u32>,
    absolute: Option<u32>,
) -> AnimeEpisode {
    AnimeEpisode::new(
        WorkId::new(id).unwrap(),
        titles("ja", title),
        class,
        aired,
        absolute,
    )
    .unwrap()
}

fn anime() -> AnimeSeries {
    AnimeSeries::new(
        WorkId::new("frieren").unwrap(),
        titles("ja", "葬送のフリーレン"),
        AnimeSeriesRelation::Adaptation,
        vec![
            Cour::new(
                1,
                vec![
                    episode(
                        "episode-1",
                        "冒険の終わり",
                        AnimeEpisodeClass::Regular,
                        Some(1),
                        Some(1),
                    ),
                    episode("ova-1", "特別編", AnimeEpisodeClass::Ova, Some(1), Some(29)),
                    episode("ona-1", "配信編", AnimeEpisodeClass::Ona, Some(1), Some(30)),
                    episode(
                        "special-1",
                        "旅立ちの記録",
                        AnimeEpisodeClass::Special,
                        Some(1),
                        None,
                    ),
                ],
            )
            .unwrap(),
            Cour::new(
                2,
                vec![episode(
                    "episode-29",
                    "再会",
                    AnimeEpisodeClass::Regular,
                    Some(1),
                    Some(31),
                )],
            )
            .unwrap(),
        ],
    )
}

#[test]
fn plans_cours_and_class_qualified_episode_sidecars() {
    let resolved = Resolved {
        value: anime(),
        provenance: ProvenanceMap::new(),
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings: Vec::new(),
    };
    let plan = AnimeWriter.plan_resolved(&resolved, "library").unwrap();
    let targets = plan
        .operations()
        .iter()
        .filter_map(OutputOperation::target)
        .map(PathBuf::from)
        .collect::<Vec<_>>();

    assert_eq!(
        targets,
        vec![
            PathBuf::from("anime.nfo"),
            PathBuf::from("Cour 01/cour.nfo"),
            PathBuf::from("Cour 01/C01E001.nfo"),
            PathBuf::from("Cour 01/C01OVA001.nfo"),
            PathBuf::from("Cour 01/C01ONA001.nfo"),
            PathBuf::from("Cour 01/C01SP001.nfo"),
            PathBuf::from("Cour 02/cour.nfo"),
            PathBuf::from("Cour 02/C02E001.nfo"),
        ]
    );

    let ova = plan
        .operations()
        .iter()
        .find(|operation| operation.target() == Some(Path::new("Cour 01/C01OVA001.nfo")))
        .unwrap();
    let OutputOperation::WriteBytes { content, .. } = ova else {
        panic!("expected sidecar write");
    };
    let xml = String::from_utf8(content.as_bytes().to_vec()).unwrap();
    assert!(xml.contains("<episodeclass>ova</episodeclass>"));
    assert!(xml.contains("<airednumber>1</airednumber>"));
    assert!(xml.contains("<absolutenumber>29</absolutenumber>"));
    assert!(xml.contains("<cour>1</cour>"));
}

#[test]
fn absolute_only_episode_uses_an_explicit_absolute_path() {
    let mut anime = anime();
    anime.cours = vec![
        Cour::new(
            3,
            vec![episode(
                "absolute-42",
                "総集編",
                AnimeEpisodeClass::Special,
                None,
                Some(42),
            )],
        )
        .unwrap(),
    ];
    let plan = AnimeWriter
        .plan(WriteRequest::new(
            MetadataDocument::Anime(anime),
            "library".into(),
        ))
        .unwrap();
    assert_eq!(
        plan.operations()[2].target(),
        Some(Path::new("Cour 03/C03SPA0042.nfo"))
    );
}

#[test]
fn writer_trait_dispatches_anime_documents() {
    let plan = AnimeWriter
        .plan(WriteRequest::new(
            MetadataDocument::Anime(anime()),
            "library".into(),
        ))
        .unwrap();
    assert_eq!(plan.operations()[0].target(), Some(Path::new("anime.nfo")));
}
