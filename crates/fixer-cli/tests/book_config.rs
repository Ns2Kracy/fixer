use std::{fs, process::Command};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

#[test]
fn provider_list_advertises_local_books_and_openlibrary() {
    let output = fixer().args(["providers", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("local\tmovie,television,anime,music,book\toffline"));
    assert!(stdout.contains("openlibrary\tbook\tnetwork"));
}

#[test]
fn config_file_accepts_openlibrary_api_and_cover_endpoint_overrides() {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("fixer.json");
    fs::write(
        &config,
        r#"{
          "openlibrary_base_url": "http://127.0.0.1:9101/",
          "openlibrary_cover_base_url": "http://127.0.0.1:9102/covers/"
        }"#,
    )
    .unwrap();

    let output = fixer()
        .current_dir(root.path())
        .arg("--config")
        .arg(config)
        .args(["config", "validate"])
        .env_remove("OPENLIBRARY_BASE_URL")
        .env_remove("OPENLIBRARY_COVER_BASE_URL")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("openlibrary_api: configured"));
    assert!(stdout.contains("openlibrary_cover: configured"));
    assert!(!stdout.contains("127.0.0.1"));
}
