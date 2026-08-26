use std::{fs, process::Command};

fn anime_library() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let anime = root.path().join("Frieren");
    let cour = anime.join("Cour 01");
    fs::create_dir_all(&cour).unwrap();
    fs::write(
        anime.join("anime.nfo"),
        "<anime><title>葬送のフリーレン</title><relation>adaptation</relation></anime>",
    )
    .unwrap();
    fs::write(
        cour.join("cour.nfo"),
        "<courdetails><cour>1</cour></courdetails>",
    )
    .unwrap();
    fs::write(
        cour.join("C01E001.nfo"),
        "<episodedetails><title>冒険の終わり</title><cour>1</cour><episodeclass>regular</episodeclass><airednumber>1</airednumber><absolutenumber>1</absolutenumber></episodedetails>",
    )
    .unwrap();
    root
}

#[test]
fn anime_scrape_previews_cour_hierarchy_in_place() {
    let root = anime_library();
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .arg("--offline")
        .arg("scrape")
        .arg(root.path().join("Frieren"))
        .args(["--kind", "anime", "--dry-run"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("planned 3 operation(s)")
    );
}

#[test]
fn anime_scrape_rejects_media_relocation_until_video_identification_exists() {
    let root = anime_library();
    let output = Command::new(env!("CARGO_BIN_EXE_fixer"))
        .arg("--offline")
        .arg("scrape")
        .arg(root.path().join("Frieren"))
        .args(["--kind", "anime", "--placement", "copy", "--dry-run"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("anime scrape currently supports only in-place placement")
    );
}
