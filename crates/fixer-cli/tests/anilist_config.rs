use std::{fs, process::Command};

#[test]
fn anilist_is_disabled_by_default() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .current_dir(root.path())
        .args(["config", "validate"])
        .env_remove("FIXER_CONFIG")
        .env_remove("FIXER_ANILIST_ENABLED")
        .env_remove("ANILIST_ACCESS_TOKEN")
        .env_remove("ANILIST_ENDPOINT")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("anilist: disabled (default)"));
}

#[test]
fn file_enables_anilist_without_exposing_token_or_endpoint() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("fixer.json");
    fs::write(
        &config,
        r#"{
          "anilist_enabled": true,
          "anilist_endpoint": "https://private.example/graphql",
          "anilist_access_token": "super-secret-token"
        }"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .current_dir(root.path())
        .arg("--config")
        .arg(config)
        .args(["config", "validate"])
        .env_remove("FIXER_ANILIST_ENABLED")
        .env_remove("ANILIST_ACCESS_TOKEN")
        .env_remove("ANILIST_ENDPOINT")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("anilist: enabled (file)"));
    assert!(!stdout.contains("super-secret-token"));
    assert!(!stdout.contains("private.example"));
}

#[test]
fn environment_can_enable_anilist_and_supply_overrides() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .current_dir(root.path())
        .args(["config", "validate"])
        .env_remove("FIXER_CONFIG")
        .env("FIXER_ANILIST_ENABLED", "true")
        .env("ANILIST_ENDPOINT", "http://127.0.0.1:9999/graphql")
        .env("ANILIST_ACCESS_TOKEN", "env-secret-token")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("anilist: enabled (environment)"));
    assert!(!stdout.contains("env-secret-token"));
    assert!(!stdout.contains("127.0.0.1"));
}
