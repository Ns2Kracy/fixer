use std::path::{Path, PathBuf};

use fixer_core::{
    AnimeSeries, AnimeSeriesRelation, BookWork, Credit, CreditRole, LocalizedValue, Movie,
    MovieRelease, MusicArtist, MusicReleaseGroup, OrderingScheme, OutputOperation, OutputPlan,
    Person, PersonId, ProvenanceMap, ReleaseDate, ReleaseId, Resolved, Season, Series, WorkId,
};
use fixer_writer_local::{
    OrganizationMedia, OrganizationPlacement, OrganizationRequest, metadata_only, organize,
};

fn titles(value: &str) -> LocalizedValue<String> {
    let mut titles = LocalizedValue::new();
    titles.insert("en", value.to_owned()).unwrap();
    titles
}

const fn resolved<T>(value: T) -> Resolved<T> {
    Resolved {
        value,
        provenance: ProvenanceMap::new(),
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings: Vec::new(),
    }
}

fn movie() -> Resolved<Movie> {
    let mut movie = Movie::new(WorkId::new("arrival").unwrap(), titles("Arrival"));
    movie.releases.push(MovieRelease::new(
        ReleaseId::new("arrival-2016").unwrap(),
        ReleaseDate::year(2016).unwrap(),
    ));
    resolved(movie)
}

fn television() -> Resolved<Series> {
    resolved(Series::new(
        WorkId::new("example-show").unwrap(),
        titles("Example Show"),
        OrderingScheme::Aired,
        vec![Season::new(WorkId::new("season-2").unwrap(), 2, Vec::new()).unwrap()],
    ))
}

fn anime() -> Resolved<AnimeSeries> {
    resolved(AnimeSeries::new(
        WorkId::new("frieren").unwrap(),
        titles("Frieren"),
        AnimeSeriesRelation::Adaptation,
        vec![fixer_core::Cour::new(3, Vec::new()).unwrap()],
    ))
}

fn music() -> Resolved<MusicReleaseGroup> {
    resolved(MusicReleaseGroup::new(
        WorkId::new("kind-of-blue").unwrap(),
        titles("Kind of Blue"),
        MusicArtist::new(WorkId::new("miles-davis").unwrap(), "Miles Davis").unwrap(),
        Vec::new(),
    ))
}

fn book() -> Resolved<BookWork> {
    let author = Person::new(
        PersonId::new("ursula-le-guin").unwrap(),
        "Ursula K. Le Guin",
    )
    .unwrap();
    resolved(BookWork::new(
        WorkId::new("left-hand-darkness").unwrap(),
        titles("The Left Hand of Darkness"),
        vec![Credit::new(author, CreditRole::Author)],
        Vec::new(),
    ))
}

fn request<'a>(
    source: &'a Path,
    media: OrganizationMedia<'a>,
    placement: OrganizationPlacement,
) -> OrganizationRequest<'a> {
    OrganizationRequest::new(
        source,
        Path::new("/library"),
        media,
        placement,
        OutputPlan::new("unused"),
    )
}

fn first_target(plan: &OutputPlan) -> &Path {
    plan.operations()[0].target().unwrap()
}

#[test]
fn movie_preset_places_media_beneath_destination() {
    let movie = movie();
    let plan = organize(request(
        Path::new("/incoming/Arrival.mkv"),
        OrganizationMedia::Movie(&movie),
        OrganizationPlacement::Hardlink,
    ))
    .unwrap();

    assert_eq!(plan.output_root, PathBuf::from("/library"));
    assert!(matches!(
        plan.operations()[0],
        OutputOperation::Hardlink { .. }
    ));
    assert_eq!(
        first_target(&plan),
        Path::new("Arrival (2016)/Arrival (2016).mkv")
    );
    assert!(
        plan.operations()
            .iter()
            .all(|operation| operation.target().unwrap().is_relative())
    );
}

#[test]
fn directory_source_places_media_recursively_without_copying_replaced_metadata() {
    let source = tempfile::tempdir().unwrap();
    let subtitles = source.path().join("Subtitles");
    std::fs::create_dir_all(&subtitles).unwrap();
    let media = source.path().join("Arrival.mkv");
    let subtitle = subtitles.join("Arrival.en.srt");
    let old_metadata = source.path().join("movie.nfo");
    std::fs::write(&media, b"movie").unwrap();
    std::fs::write(&subtitle, b"subtitle").unwrap();
    std::fs::write(&old_metadata, b"old metadata").unwrap();

    let mut metadata = OutputPlan::new("unused");
    metadata.push(
        OutputOperation::write_bytes(
            "movie.nfo",
            fixer_core::PlannedContent::new(b"new metadata"),
        )
        .unwrap(),
    );
    let movie = movie();
    let plan = organize(OrganizationRequest::new(
        source.path(),
        Path::new("/library"),
        OrganizationMedia::Movie(&movie),
        OrganizationPlacement::Copy,
        metadata,
    ))
    .unwrap();

    assert!(plan.operations().iter().any(|operation| {
        matches!(
            operation,
            OutputOperation::Copy { source, target }
                if source == &media && target == Path::new("Arrival (2016)/Arrival (2016).mkv")
        )
    }));
    assert!(plan.operations().iter().any(|operation| {
        matches!(
            operation,
            OutputOperation::Copy { source, target }
                if source == &subtitle && target == Path::new("Arrival (2016)/Subtitles/Arrival.en.srt")
        )
    }));
    assert!(!plan.operations().iter().any(|operation| {
        matches!(operation, OutputOperation::Copy { source, .. } if source == &old_metadata)
    }));
    assert!(plan.operations().iter().any(|operation| {
        matches!(
            operation,
            OutputOperation::WriteBytes { target, .. }
                if target == Path::new("Arrival (2016)/movie.nfo")
        )
    }));
}

#[test]
fn every_media_kind_has_a_bounded_builtin_layout() {
    let television = television();
    let anime = anime();
    let music = music();
    let book = book();
    let cases = [
        (
            organize(request(
                Path::new("/incoming/Episode.S02E01.mkv"),
                OrganizationMedia::Television(&television),
                OrganizationPlacement::Copy,
            ))
            .unwrap(),
            Path::new("Example Show/Season 02/Episode.S02E01.mkv"),
        ),
        (
            organize(request(
                Path::new("/incoming/Frieren.C03E01.mkv"),
                OrganizationMedia::Anime(&anime),
                OrganizationPlacement::Copy,
            ))
            .unwrap(),
            Path::new("Frieren/Cour 03/Frieren.C03E01.mkv"),
        ),
        (
            organize(request(
                Path::new("/incoming/01 - So What.flac"),
                OrganizationMedia::Music(&music),
                OrganizationPlacement::Copy,
            ))
            .unwrap(),
            Path::new("Miles Davis/Kind of Blue/01 - So What.flac"),
        ),
        (
            organize(request(
                Path::new("/incoming/left-hand.epub"),
                OrganizationMedia::Book(&book),
                OrganizationPlacement::Copy,
            ))
            .unwrap(),
            Path::new("Ursula K. Le Guin/The Left Hand of Darkness/The Left Hand of Darkness.epub"),
        ),
    ];

    for (plan, expected) in cases {
        assert_eq!(plan.output_root, PathBuf::from("/library"));
        assert_eq!(first_target(&plan), expected);
        assert_eq!(first_target(&plan).extension(), expected.extension());
        assert!(first_target(&plan).is_relative());
        assert!(
            !first_target(&plan)
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        );
    }
}

#[test]
fn custom_template_is_relative_and_preserves_the_source_extension() {
    let movie = movie();
    let plan = organize(
        request(
            Path::new("/incoming/Arrival.remux.mkv"),
            OrganizationMedia::Movie(&movie),
            OrganizationPlacement::Copy,
        )
        .with_path_template("Curated/{{ title | sanitize }}"),
    )
    .unwrap();
    assert_eq!(
        first_target(&plan),
        Path::new("Curated/Arrival/Arrival.mkv")
    );

    for template in [
        "/escape/{{ title }}",
        "../escape/{{ title }}",
        "safe/../../escape",
    ] {
        let error = organize(
            request(
                Path::new("/incoming/Arrival.mkv"),
                OrganizationMedia::Movie(&movie),
                OrganizationPlacement::Copy,
            )
            .with_path_template(template),
        )
        .unwrap_err();
        assert!(error.to_string().contains("unsafe"));
    }
}

#[test]
fn every_placement_maps_to_its_exact_operation() {
    let movie = movie();
    let cases = [
        (OrganizationPlacement::Move, "move"),
        (OrganizationPlacement::Copy, "copy"),
        (OrganizationPlacement::Hardlink, "hardlink"),
        (OrganizationPlacement::Symlink, "symlink"),
        (OrganizationPlacement::Reflink, "reflink"),
    ];

    for (placement, expected) in cases {
        let plan = organize(request(
            Path::new("/incoming/Arrival.mkv"),
            OrganizationMedia::Movie(&movie),
            placement,
        ))
        .unwrap();
        let actual = match &plan.operations()[0] {
            OutputOperation::Move { .. } => "move",
            OutputOperation::Copy { .. } => "copy",
            OutputOperation::Hardlink { .. } => "hardlink",
            OutputOperation::Symlink { .. } => "symlink",
            OutputOperation::Reflink { .. } => "reflink",
            operation => panic!("unexpected placement operation: {operation:?}"),
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn metadata_operations_are_rebased_and_manifest_entries_are_reconciled() {
    let movie = movie();
    let mut metadata = OutputPlan::new("old-root");
    metadata.push(
        OutputOperation::write_bytes("movie.json", fixer_core::PlannedContent::new(b"{}")).unwrap(),
    );
    metadata.push(
        OutputOperation::write_bytes(
            "fixer-manifest.json",
            fixer_core::PlannedContent::new(br#"{"planned_files":["movie.json","cover.jpg"]}"#),
        )
        .unwrap(),
    );
    metadata.push(OutputOperation::copy("/incoming/cover.jpg", "cover.jpg").unwrap());
    let metadata = metadata_only(&metadata).unwrap();
    let plan = organize(OrganizationRequest::new(
        Path::new("/incoming/Arrival.mkv"),
        Path::new("/library"),
        OrganizationMedia::Movie(&movie),
        OrganizationPlacement::Copy,
        metadata,
    ))
    .unwrap();

    assert_eq!(plan.operations().len(), 3);
    assert_eq!(
        plan.operations()[1].target(),
        Some(Path::new("Arrival (2016)/movie.json"))
    );
    let OutputOperation::WriteBytes { content, .. } = &plan.operations()[2] else {
        panic!("expected manifest write");
    };
    let manifest: serde_json::Value = serde_json::from_slice(content.as_bytes()).unwrap();
    assert_eq!(manifest["planned_files"], serde_json::json!(["movie.json"]));
}

#[test]
fn directory_episodes_and_matching_nfos_and_subtitles_follow_each_season() {
    let source = tempfile::tempdir().unwrap();
    std::fs::create_dir(source.path().join("nested")).unwrap();
    let mut metadata = OutputPlan::new("unused");
    for (directory, season, episode) in [("", 1, 1), ("nested/", 2, 2)] {
        let stem = format!("Example.Show.S{season:02}E{episode:02}.1080p");
        for extension in ["mkv", "en.srt"] {
            std::fs::write(
                source.path().join(format!("{directory}{stem}.{extension}")),
                [],
            )
            .unwrap();
        }
        metadata.push(
            OutputOperation::write_bytes(
                format!("Season {season:02}/S{season:02}E{episode:02}.nfo"),
                fixer_core::PlannedContent::new(b"episode metadata"),
            )
            .unwrap(),
        );
    }
    let television = television();
    let plan = organize(OrganizationRequest::new(
        source.path(),
        Path::new("/library"),
        OrganizationMedia::Television(&television),
        OrganizationPlacement::Copy,
        metadata,
    ))
    .unwrap();
    for (season, episode) in [(1, 1), (2, 2)] {
        for extension in ["mkv", "en.srt", "nfo"] {
            let target = PathBuf::from(format!(
                "Example Show/Season {season:02}/Example.Show.S{season:02}E{episode:02}.1080p.{extension}"
            ));
            assert!(
                plan.operations()
                    .iter()
                    .any(|op| op.target() == Some(target.as_path())),
                "missing {target:?}"
            );
        }
    }
}

#[test]
fn duplicate_episode_mapping_is_rejected() {
    let source = tempfile::tempdir().unwrap();
    for name in ["Show.S01E01.mkv", "Show.S01E01.mp4"] {
        std::fs::write(source.path().join(name), []).unwrap();
    }
    let television = television();
    assert!(
        organize(request(
            source.path(),
            OrganizationMedia::Television(&television),
            OrganizationPlacement::Copy
        ))
        .is_err()
    );
}

#[test]
fn ten_flat_episodes_use_season_one_not_first_metadata_season() {
    let source = tempfile::tempdir().unwrap();
    let television = television(); // metadata's first season is deliberately 2
    for episode in 1..=10 {
        std::fs::write(source.path().join(format!("Show.S01E{episode:02}.mkv")), []).unwrap();
    }
    let plan = organize(request(
        source.path(),
        OrganizationMedia::Television(&television),
        OrganizationPlacement::Copy,
    ))
    .unwrap();
    assert_eq!(plan.operations().len(), 10);
    for episode in 1..=10 {
        let expected = PathBuf::from(format!("Example Show/Season 01/Show.S01E{episode:02}.mkv"));
        assert!(
            plan.operations()
                .iter()
                .any(|operation| operation.target() == Some(expected.as_path()))
        );
    }
}

#[test]
fn ambiguous_multi_episode_and_unidentified_files_are_not_assigned_the_first_season() {
    let television = television();
    for name in [
        "Show.S01E01E02.mkv",
        "Show.S01E01-E02.mkv",
        "Show.S01E01-02.mkv",
        "Show.S01E01.S02E02.mkv",
        "unknown.mkv",
    ] {
        let plan = organize(request(
            Path::new(name),
            OrganizationMedia::Television(&television),
            OrganizationPlacement::Copy,
        ))
        .unwrap();
        assert_eq!(first_target(&plan), Path::new("Example Show").join(name));
    }
}

#[test]
fn season_folder_episode_and_unique_subtitle_are_colocated_but_ambiguous_subtitle_is_preserved() {
    let source = tempfile::tempdir().unwrap();
    for season in [1, 2] {
        let folder = source.path().join(format!("Season {season:02}"));
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("01 - Pilot.mkv"), []).unwrap();
    }
    std::fs::create_dir(source.path().join("Subtitles")).unwrap();
    std::fs::write(source.path().join("Subtitles/01 - Pilot.en.srt"), []).unwrap();
    let television = television();
    let plan = organize(request(
        source.path(),
        OrganizationMedia::Television(&television),
        OrganizationPlacement::Copy,
    ))
    .unwrap();
    assert!(plan.operations().iter().any(|operation| operation.target()
        == Some(Path::new("Example Show/Subtitles/01 - Pilot.en.srt"))));
    for season in [1, 2] {
        let expected = PathBuf::from(format!("Example Show/Season {season:02}/01 - Pilot.mkv"));
        assert!(
            plan.operations()
                .iter()
                .any(|operation| operation.target() == Some(expected.as_path()))
        );
    }
}

#[test]
fn single_episode_nfo_uses_overridden_media_stem() {
    let television = television();
    let mut metadata = OutputPlan::new("unused");
    metadata.push(
        OutputOperation::write_bytes(
            "Season 01/S01E01.nfo",
            fixer_core::PlannedContent::new(b"nfo"),
        )
        .unwrap(),
    );
    let plan = organize(
        OrganizationRequest::new(
            Path::new("Show.S01E01.mkv"),
            Path::new("/library"),
            OrganizationMedia::Television(&television),
            OrganizationPlacement::Copy,
            metadata,
        )
        .with_media_target(Path::new("Season 01/Renamed.mkv")),
    )
    .unwrap();
    assert_eq!(
        plan.operations()[1].target().unwrap(),
        Path::new("Example Show/Season 01/Renamed.nfo")
    );
}
