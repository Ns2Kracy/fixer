use std::{fs, process::Command};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

fn album_library() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let album = root.path().join("Miles Davis/Kind of Blue");
    fs::create_dir_all(&album).unwrap();
    let mut bytes = vec![0x55; 32];
    bytes.extend_from_slice(b"TAG");
    for (value, width) in [
        ("So What", 30),
        ("Miles Davis", 30),
        ("Kind of Blue", 30),
        ("1959", 4),
        ("", 28),
    ] {
        bytes.extend_from_slice(value.as_bytes());
        bytes.resize(bytes.len() + width - value.len(), 0);
    }
    bytes.extend_from_slice(&[0, 1, 0]);
    fs::write(album.join("01 So What.mp3"), bytes).unwrap();
    root
}

#[test]
fn music_search_and_resolve_preserve_album_hierarchy_offline() {
    let root = album_library();
    let search = fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args(["search", "music", "Kind of Blue", "--year", "1959"])
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let stdout = String::from_utf8(search.stdout).unwrap();
    assert!(stdout.contains("Kind of Blue"));
    assert!(stdout.contains("local"));

    let resolve = fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "resolve",
            "music",
            "Kind of Blue",
            "--year",
            "1959",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        resolve.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "music");
    assert_eq!(value["title"], "Kind of Blue");
    assert_eq!(value["artist"], "Miles Davis");
    assert_eq!(value["releases"].as_array().unwrap().len(), 1);
    assert_eq!(value["releases"][0]["discs"].as_array().unwrap().len(), 1);
    assert_eq!(
        value["releases"][0]["discs"][0]["tracks"][0]["title"],
        "So What"
    );
    assert_eq!(value["releases"][0]["discs"][0]["tracks"][0]["track"], 1);
}

#[test]
fn music_scrape_plans_and_applies_metadata_without_mutating_audio() {
    let root = album_library();
    let album = root.path().join("Miles Davis/Kind of Blue");
    let audio = album.join("01 So What.mp3");
    let original = fs::read(&audio).unwrap();

    let dry_run = fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(&album)
        .args(["--kind", "music", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(
        dry_run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    assert!(
        String::from_utf8(dry_run.stdout)
            .unwrap()
            .contains("planned 2 operation(s)")
    );
    assert!(!album.join("album.json").exists());

    let apply = fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(&album)
        .args(["--kind", "music", "--apply"])
        .output()
        .unwrap();
    assert_eq!(
        apply.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    assert!(album.join("album.json").is_file());
    assert!(album.join("fixer-manifest.json").is_file());
    assert!(!album.join("tag-update-intent.json").exists());
    assert_eq!(fs::read(&audio).unwrap(), original);

    let placement = fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(&audio)
        .args(["--kind", "music", "--placement", "hardlink"])
        .output()
        .unwrap();
    assert_eq!(placement.status.code(), Some(2));
    assert!(
        String::from_utf8(placement.stderr)
            .unwrap()
            .contains("music scrape currently supports only in-place placement")
    );
}

#[test]
fn provider_list_advertises_musicbrainz_and_local_music() {
    let output = fixer().args(["providers", "list"]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("local\tmovie,television,anime,music,book\toffline"));
    assert!(stdout.contains("musicbrainz\tmusic\tnetwork"));
}
