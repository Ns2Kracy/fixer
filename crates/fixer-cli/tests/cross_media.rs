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
        (
            r#"{"enabled_providers":["bangumi"],"bangumi_base_url":"not a URL"}"#,
            "URL",
        ),
        (r#"{"proxy":"not a proxy URL"}"#, "proxy"),
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

#[test]
fn plan_emits_a_stable_operation_summary_without_mutating_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("source.json"),
        include_str!("../../fixer-provider-local/tests/fixtures/movie.json"),
    )
    .unwrap();
    let output = fixer()
        .arg("--offline")
        .arg("plan")
        .arg(root.path())
        .args(["--kind", "movie", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "movie");
    assert_eq!(value["output_root"], root.path().to_string_lossy().as_ref());
    let operations = value["operations"].as_array().unwrap();
    assert!(!operations.is_empty());
    for operation in operations {
        assert!(operation["operation"].is_string());
        assert!(operation["target"].is_string());
        assert!(operation.get("content").is_none());
        assert!(operation.get("bytes").is_none());
    }
    assert!(!root.path().join("movie.json").exists());
    assert!(!root.path().join("fixer-manifest.json").exists());
}

#[test]
fn plan_cannot_accept_execution_flags() {
    let root = tempfile::tempdir().unwrap();
    let output = fixer()
        .arg("plan")
        .arg(root.path())
        .args(["--kind", "movie", "--apply"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unexpected argument '--apply'")
    );
}

fn conflicting_movie_library(
    policy: &str,
) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    fs::create_dir(&library).unwrap();
    for (name, id, summary) in [
        ("first.json", "first", "First summary"),
        ("second.json", "second", "Second summary"),
    ] {
        fs::write(
            library.join(name),
            format!(
                r#"{{
                  "id": {{ "namespace": "local", "value": "{id}" }},
                  "titles": {{ "en": "Conflicted Movie" }},
                  "year": 2020,
                  "summary": {{ "en": "{summary}" }}
                }}"#,
            ),
        )
        .unwrap();
    }
    let config = root.path().join("fixer.json");
    fs::write(
        &config,
        format!(r#"{{"enabled_providers":["local"],"conflict_policy":"{policy}"}}"#),
    )
    .unwrap();
    (root, config, library)
}

#[test]
fn review_conflict_policy_returns_review_exit_and_blocks_apply() {
    let (_root, config, library) = conflicting_movie_library("review");
    let output = fixer()
        .args(["--config", config.to_str().unwrap(), "--offline", "scrape"])
        .arg(&library)
        .args(["--kind", "movie", "--apply"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("review required")
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("planned 1 operation(s)"), "{stdout}");
    assert!(stdout.contains("write_bytes movie.json"), "{stdout}");
    assert!(!library.join("movie.json").exists());
    assert!(!library.join("fixer-manifest.json").exists());
}

#[test]
fn conflict_policy_can_prefer_first_or_fail() {
    let (_preferred_root, preferred_config, preferred_library) =
        conflicting_movie_library("prefer_first");
    let preferred = fixer()
        .args([
            "--config",
            preferred_config.to_str().unwrap(),
            "--offline",
            "plan",
        ])
        .arg(&preferred_library)
        .args(["--kind", "movie", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        preferred.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&preferred.stderr)
    );

    let (_error_root, error_config, error_library) = conflicting_movie_library("error");
    let failed = fixer()
        .args([
            "--config",
            error_config.to_str().unwrap(),
            "--offline",
            "plan",
        ])
        .arg(&error_library)
        .args(["--kind", "movie"])
        .output()
        .unwrap();
    assert_eq!(failed.status.code(), Some(1));
    assert!(
        String::from_utf8(failed.stderr)
            .unwrap()
            .contains("conflict policy rejected 1 metadata conflict")
    );
}

#[test]
fn semantic_invalid_input_returns_usage_exit_code() {
    let root = tempfile::tempdir().unwrap();
    let missing = root.path().join("missing");
    let scan = fixer()
        .arg("scan")
        .arg(&missing)
        .args(["--kind", "movie"])
        .output()
        .unwrap();
    assert_eq!(scan.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&scan.stderr).contains("does not exist"));

    let invalid_kind_option = fixer()
        .arg("scrape")
        .arg(root.path())
        .args(["--kind", "movie", "--update-epub"])
        .output()
        .unwrap();
    assert_eq!(invalid_kind_option.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid_kind_option.stderr).contains("only for book"));
}

#[test]
fn configured_placement_is_used_when_the_cli_flag_is_absent() {
    let root = tempfile::tempdir().unwrap();
    let library = root.path().join("library");
    fs::create_dir(&library).unwrap();
    let media = library.join("Example.Movie.2020.mkv");
    fs::write(&media, b"movie").unwrap();
    let config = root.path().join("fixer.json");
    fs::write(
        &config,
        r#"{"enabled_providers":["local"],"placement":"copy"}"#,
    )
    .unwrap();

    let output = fixer()
        .args(["--config", config.to_str().unwrap(), "--offline", "plan"])
        .arg(&media)
        .args(["--kind", "movie", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        value["output_root"]
            .as_str()
            .unwrap()
            .ends_with("Example Movie (2020)")
    );
    assert!(
        value["operations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|operation| operation["operation"] == "copy")
    );
    assert!(!library.join("Example Movie (2020)").exists());
}
