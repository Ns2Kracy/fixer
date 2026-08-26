use std::{
    fs,
    process::{Command, Output},
};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}
fn run(command: &mut Command) -> Output {
    command.output().unwrap()
}
fn fixture() -> &'static str {
    include_str!("../../fixer-provider-local/tests/fixtures/movie.json")
}

fn json_keys(value: &serde_json::Value) -> std::collections::BTreeSet<&str> {
    value
        .as_object()
        .expect("JSON contract value must be an object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn help_lists_the_first_cli_surface() {
    let output = run(fixer().arg("--help"));
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for command in ["search", "resolve", "scrape", "config", "providers"] {
        assert!(stdout.contains(command));
    }
}

#[test]
fn move_is_not_a_valid_placement() {
    let output = run(fixer().args(["scrape", ".", "--kind", "movie", "--placement", "move"]));
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("invalid value")
    );
}

#[test]
fn dry_run_and_apply_are_mutually_exclusive() {
    let output = run(fixer().args(["scrape", ".", "--kind", "movie", "--dry-run", "--apply"]));
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn resolve_json_uses_a_stable_dto() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.json"), fixture()).unwrap();
    let output = run(fixer().arg("--local-root").arg(root.path()).args([
        "resolve",
        "movie",
        "花样年华",
        "--year",
        "2000",
        "--json",
    ]));
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json_keys(&value),
        std::collections::BTreeSet::from([
            "completeness",
            "conflicts",
            "id",
            "kind",
            "schema_version",
            "title",
            "titles",
            "warnings",
            "year",
        ])
    );
    assert_eq!(
        json_keys(&value["titles"][0]),
        std::collections::BTreeSet::from(["locale", "value"])
    );
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "movie");
    assert_eq!(value["title"], "花样年华");
    assert_eq!(value["year"], 2000);
    assert!(value["id"].is_string());
    assert!(value["titles"].is_array());
}

#[test]
fn config_validate_redacts_secrets_and_reports_flag_precedence() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("fixer.json");
    fs::write(
        &config,
        r#"{"proxy":"http://file.invalid","api_key":"file-secret"}"#,
    )
    .unwrap();
    let output = run(fixer()
        .arg("--config")
        .arg(&config)
        .arg("--proxy")
        .arg("http://flag.invalid")
        .arg("config")
        .arg("validate")
        .env("FIXER_PROXY", "http://env.invalid")
        .env("FIXER_API_KEY", "env-secret"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("proxy: configured (flag)"));
    assert!(stdout.contains("api_key: configured (environment)"));
    for secret in [
        "file-secret",
        "env-secret",
        "file.invalid",
        "env.invalid",
        "flag.invalid",
    ] {
        assert!(!stdout.contains(secret));
    }
}

#[test]
fn scrape_warnings_return_partial_success_exit_code() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.json"), fixture()).unwrap();
    fs::write(root.path().join("broken.nfo"), "<movie><title>broken").unwrap();
    let output = run(fixer()
        .arg("scrape")
        .arg(root.path())
        .args(["--kind", "movie", "--dry-run"]));
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("broken.nfo")
    );
}

#[test]
fn television_search_resolve_and_scrape_work_offline() {
    let root = tempfile::tempdir().unwrap();
    let season = root.path().join("Example Show").join("Season 01");
    fs::create_dir_all(&season).unwrap();
    let episode_path = season.join("Example.Show.S01E02.{tmdb-1399}.mkv");
    fs::write(&episode_path, []).unwrap();

    let search = run(fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "search",
            "television",
            "Example Show",
            "--external-id",
            "tmdb:1399",
        ]));
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(
        String::from_utf8(search.stdout)
            .unwrap()
            .contains("Example Show")
    );

    let resolve = run(fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "resolve",
            "television",
            "Example Show",
            "--external-id",
            "tmdb:1399",
            "--ordering",
            "aired",
            "--json",
        ]));
    assert_eq!(
        resolve.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(
        json_keys(&value),
        std::collections::BTreeSet::from([
            "completeness",
            "conflicts",
            "episodes",
            "id",
            "kind",
            "ordering",
            "schema_version",
            "seasons",
            "title",
            "titles",
            "warnings",
        ])
    );
    assert_eq!(
        json_keys(&value["titles"][0]),
        std::collections::BTreeSet::from(["locale", "value"])
    );
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "television");
    assert_eq!(value["title"], "Example Show");
    assert_eq!(value["ordering"], "aired");
    assert_eq!(value["seasons"], 1);
    assert_eq!(value["episodes"], 1);

    let scrape = run(fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(root.path())
        .args(["--kind", "television", "--dry-run"]));
    assert_eq!(
        scrape.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&scrape.stderr)
    );
    assert!(
        String::from_utf8(scrape.stdout)
            .unwrap()
            .contains("planned 3 operation(s)")
    );

    let apply = run(fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(&episode_path)
        .args(["--kind", "television", "--apply"]));
    assert_eq!(
        apply.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let series_root = root.path().join("Example Show");
    assert!(series_root.join("tvshow.nfo").is_file());
    assert!(series_root.join("Season 01/season.nfo").is_file());
    assert!(series_root.join("Season 01/S01E02.nfo").is_file());
    assert!(!season.join("tvshow.nfo").exists());
    assert!(!season.join("Season 01").exists());
}

#[test]
fn television_copy_placement_keeps_media_in_its_season_directory() {
    let root = tempfile::tempdir().unwrap();
    let incoming = root.path().join("Incoming").join("Season 01");
    fs::create_dir_all(&incoming).unwrap();
    let source = incoming.join("Example.Show.S01E02.mkv");
    fs::write(&source, b"episode").unwrap();
    fs::write(
        source.with_extension("tags.xml"),
        r#"<Tags><Tag>
            <Simple><Name>SEASON</Name><String>2</String></Simple>
            <Simple><Name>EPISODE</Name><String>2</String></Simple>
        </Tag></Tags>"#,
    )
    .unwrap();

    let output = run(fixer().arg("--offline").arg("scrape").arg(&source).args([
        "--kind",
        "television",
        "--placement",
        "copy",
        "--apply",
    ]));

    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let target = root
        .path()
        .join("Example Show")
        .join("Season 02")
        .join("Example.Show.S01E02.mkv");
    assert_eq!(fs::read(target).unwrap(), b"episode");
    assert!(
        root.path()
            .join("Example Show/Season 02/S02E02.nfo")
            .is_file()
    );
    assert!(
        !root
            .path()
            .join("Example Show/Example.Show.S01E02.mkv")
            .exists()
    );
}

#[test]
fn television_scrape_rejects_roots_with_multiple_series() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("First Show").join("Season 01");
    let second = root.path().join("Second Show").join("Season 01");
    fs::create_dir_all(&first).unwrap();
    fs::create_dir_all(&second).unwrap();
    fs::write(first.join("First.Show.S01E01.mkv"), []).unwrap();
    fs::write(second.join("Second.Show.S01E01.mkv"), []).unwrap();

    let output = run(fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(root.path())
        .args(["--kind", "television", "--dry-run"]));

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("ambiguous television input: found 2 series")
    );
}

#[test]
fn apply_defaults_to_no_overwrite() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.json"), fixture()).unwrap();
    fs::write(root.path().join("movie.json"), "existing").unwrap();
    let output = run(fixer()
        .arg("scrape")
        .arg(root.path())
        .args(["--kind", "movie", "--apply"]));
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(root.path().join("movie.json")).unwrap(),
        "existing"
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("already exists")
    );
}
