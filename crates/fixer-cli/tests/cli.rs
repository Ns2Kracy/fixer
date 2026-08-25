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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
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
