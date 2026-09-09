use fixer_core::{
    OperationOutcome, OperationReport, OutputFingerprint, OutputOperationKind, ReplacementManifest,
};

fn fingerprint(value: char) -> OutputFingerprint {
    OutputFingerprint::new(value.to_string().repeat(64)).unwrap()
}

#[test]
fn operation_reports_serialize_every_audit_field() {
    let report = OperationReport::new(
        3,
        OutputOperationKind::Copy,
        Some("/incoming/Arrival.mkv".to_owned()),
        "/library/Arrival/Arrival.mkv",
        OperationOutcome::Succeeded,
        Some(fingerprint('a')),
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(report).unwrap(),
        serde_json::json!({
            "operation_index": 3,
            "kind": "copy",
            "source": "/incoming/Arrival.mkv",
            "destination": "/library/Arrival/Arrival.mkv",
            "outcome": "succeeded",
            "fingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        })
    );
}

#[test]
fn replacement_manifest_requires_an_exact_destination_and_fingerprint() {
    let expected = fingerprint('a');
    let changed = fingerprint('b');
    let reports = [
        OperationReport::new(
            0,
            OutputOperationKind::WriteBytes,
            None,
            "/library/Arrival/movie.json",
            OperationOutcome::Succeeded,
            Some(expected.clone()),
        )
        .unwrap(),
        OperationReport::new(
            1,
            OutputOperationKind::Copy,
            Some("/incoming/Arrival.mkv".to_owned()),
            "/library/Arrival/Arrival.mkv",
            OperationOutcome::Failed,
            None,
        )
        .unwrap(),
    ];

    let manifest = ReplacementManifest::from_reports(&reports).unwrap();

    assert!(manifest.allows("/library/Arrival/movie.json", &expected));
    assert!(!manifest.allows("/library/Arrival/movie.json", &changed));
    assert!(!manifest.allows("/library/Arrival/Arrival.mkv", &expected));
    assert!(!manifest.allows("/library/Unknown/file.mkv", &expected));
}

#[test]
fn audit_values_are_bounded_and_fingerprints_are_validated() {
    assert!(OutputFingerprint::new("not-a-sha256").is_err());
    assert!(
        OperationReport::new(
            0,
            OutputOperationKind::Move,
            None,
            "",
            OperationOutcome::Failed,
            None,
        )
        .is_err()
    );
}
