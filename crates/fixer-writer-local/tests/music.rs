use fixer_core::{
    AssetId, Disc, Duration, LocalizedValue, MusicArtist, MusicRelease, MusicReleaseGroup,
    OutputOperation, ProvenanceMap, ReleaseId, Resolved, Track, TrackSequence, WorkId,
};
use fixer_writer_local::MusicWriter;
use std::{collections::BTreeMap, path::PathBuf};

fn resolved_album() -> Resolved<MusicReleaseGroup> {
    let mut group_titles = LocalizedValue::new();
    group_titles
        .insert("und", "Kind of Blue".to_owned())
        .unwrap();
    let mut track_titles = LocalizedValue::new();
    track_titles.insert("und", "So What".to_owned()).unwrap();
    let track = Track::new(
        AssetId::new("recording-so-what").unwrap(),
        track_titles,
        TrackSequence::new(1, 1).unwrap(),
        Duration::from_seconds(562),
    );
    Resolved {
        value: MusicReleaseGroup::new(
            WorkId::new("release-group-kind-of-blue").unwrap(),
            group_titles,
            MusicArtist::new(WorkId::new("artist-miles-davis").unwrap(), "Miles Davis").unwrap(),
            vec![MusicRelease::new(
                ReleaseId::new("release-kind-of-blue-1959").unwrap(),
                vec![Disc::new(1, vec![track]).unwrap()],
            )],
        ),
        provenance: ProvenanceMap::new(),
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings: Vec::new(),
    }
}

fn contents(plan: &fixer_core::OutputPlan) -> BTreeMap<PathBuf, String> {
    plan.operations()
        .iter()
        .map(|operation| match operation {
            OutputOperation::WriteBytes { target, content } => (
                target.clone(),
                String::from_utf8(content.as_bytes().to_vec()).unwrap(),
            ),
            other => panic!("music writer must only declare files, got {other:?}"),
        })
        .collect()
}

#[test]
fn album_json_and_manifest_are_deterministic_and_planning_only() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("not-created");

    let first = MusicWriter::default()
        .plan_resolved(&resolved_album(), &target)
        .unwrap();
    let second = MusicWriter::default()
        .plan_resolved(&resolved_album(), &target)
        .unwrap();

    assert_eq!(first, second);
    assert!(!target.exists());
    let files = contents(&first);
    assert_eq!(files.len(), 2);
    assert!(files.contains_key(&PathBuf::from("album.json")));
    let manifest = &files[&PathBuf::from("fixer-manifest.json")];
    assert!(manifest.contains("release-group-kind-of-blue"));
    assert!(manifest.contains("album.json"));
    assert!(!manifest.contains("tag-update-intent.json"));
}

#[test]
fn explicit_tag_targets_create_confirmation_gated_intents_with_hardlink_warning() {
    let mut targets = BTreeMap::new();
    targets.insert(
        "recording-so-what".to_owned(),
        PathBuf::from("/library/Miles Davis/Kind of Blue/01 So What.mp3"),
    );
    let plan = MusicWriter::with_tag_targets(targets)
        .plan_resolved(&resolved_album(), "output")
        .unwrap();

    let files = contents(&plan);
    let intent = &files[&PathBuf::from("tag-update-intent.json")];
    assert!(intent.contains("\"requires_confirmation\": true"));
    assert!(intent.contains("01 So What.mp3"));
    assert!(intent.contains("Miles Davis"));
    assert!(intent.contains("Kind of Blue"));
    assert!(intent.contains("So What"));
    assert!(intent.contains("hardlink"));
    assert!(intent.contains("all hardlinked paths"));
    let manifest = &files[&PathBuf::from("fixer-manifest.json")];
    assert!(manifest.contains("tag-update-intent.json"));

    for operation in plan.operations() {
        let target = operation.target().unwrap();
        assert!(!target.to_string_lossy().ends_with(".mp3"));
    }
}

#[test]
fn unknown_tag_target_identity_is_rejected_instead_of_becoming_an_implicit_write() {
    let mut targets = BTreeMap::new();
    targets.insert("unknown-track".to_owned(), PathBuf::from("audio.mp3"));

    let error = MusicWriter::with_tag_targets(targets)
        .plan_resolved(&resolved_album(), "output")
        .unwrap_err();

    assert!(error.to_string().contains("unknown track identity"));
}
