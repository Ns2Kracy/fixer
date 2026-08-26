use std::{fs, process::Command};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

fn validate(config: &str) -> std::process::Output {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("fixer.json");
    fs::write(&path, config).unwrap();
    fixer()
        .current_dir(root.path())
        .arg("--config")
        .arg(path)
        .args(["config", "validate"])
        .env("FIXER_TEST_TMDB_TOKEN", "tmdb-secret-value")
        .env("FIXER_TEST_ANILIST_TOKEN", "anilist-secret-value")
        .output()
        .unwrap()
}

#[test]
fn config_validates_and_reports_the_cross_media_policy_schema() {
    let output = validate(
        r#"{
          "preferred_locales": ["ja", "en", "und"],
          "timeout_seconds": 17,
          "auto_accept_confidence": 0.9,
          "review_confidence": 0.6,
          "output_preset": "full",
          "placement": "copy",
          "conflict_policy": "review",
          "enabled_providers": ["local", "openlibrary"],
          "secret_references": {
            "tmdb_api_token": "FIXER_TEST_TMDB_TOKEN",
            "anilist_access_token": "FIXER_TEST_ANILIST_TOKEN"
          }
        }"#,
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in [
        "preferred_locales: ja,en,und",
        "timeout_seconds: 17",
        "auto_accept_confidence: 0.9",
        "review_confidence: 0.6",
        "output_preset: full",
        "placement: copy",
        "conflict_policy: review",
        "enabled_providers: local,openlibrary",
        "tmdb_secret: configured",
        "anilist_secret: configured",
    ] {
        assert!(stdout.contains(line), "missing `{line}` in {stdout}");
    }
    assert!(!stdout.contains("tmdb-secret-value"));
    assert!(!stdout.contains("anilist-secret-value"));
    assert!(!format!("{output:?}").contains("secret-value"));
}

#[test]
fn invalid_cross_media_policy_values_fail_during_config_validation() {
    for (config, expected) in [
        (r#"{"preferred_locales":["not_a_tag"]}"#, "BCP 47"),
        (r#"{"timeout_seconds":0}"#, "timeout_seconds"),
        (
            r#"{"auto_accept_confidence":0.5,"review_confidence":0.8}"#,
            "review_confidence",
        ),
        (r#"{"enabled_providers":["unknown"]}"#, "unknown provider"),
    ] {
        let output = validate(config);
        assert_eq!(
            output.status.code(),
            Some(2),
            "unexpected status for {config}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8(output.stderr).unwrap().contains(expected),
            "missing {expected} for {config}"
        );
    }
}

#[test]
fn scan_dispatches_all_media_kinds_with_a_stable_json_envelope() {
    for kind in ["anime", "book", "movie", "music", "television"] {
        let root = tempfile::tempdir().unwrap();
        let output = fixer()
            .args([
                "scan",
                root.path().to_str().unwrap(),
                "--kind",
                kind,
                "--json",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{kind}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["kind"], kind);
        assert_eq!(value["documents"], 0);
        assert_eq!(value["warnings"], serde_json::json!([]));
        assert!(value["root"].as_str().unwrap().starts_with('/'));
    }
}

#[test]
fn movie_scan_counts_local_documents_without_serializing_domain_models() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("movie.json"),
        include_str!("../../fixer-provider-local/tests/fixtures/movie.json"),
    )
    .unwrap();
    let output = fixer()
        .args([
            "scan",
            root.path().to_str().unwrap(),
            "--kind",
            "movie",
            "--json",
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["documents"], 1);
    assert!(value.get("id").is_none());
    assert!(value.get("titles").is_none());
}

#[test]
fn scan_warnings_are_structured_and_return_partial_success() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("broken.json"), b"not json").unwrap();
    let output = fixer()
        .args([
            "scan",
            root.path().to_str().unwrap(),
            "--kind",
            "movie",
            "--json",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["documents"], 0);
    assert_eq!(value["warnings"].as_array().unwrap().len(), 1);
    assert!(
        value["warnings"][0]["path"]
            .as_str()
            .unwrap()
            .ends_with("broken.json")
    );
    assert!(
        value["warnings"][0]["message"]
            .as_str()
            .unwrap()
            .contains("invalid")
    );
}
