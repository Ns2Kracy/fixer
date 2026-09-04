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
