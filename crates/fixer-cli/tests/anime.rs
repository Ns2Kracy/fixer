use std::{fs, process::Command};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

fn run(command: &mut Command) -> std::process::Output {
    command.output().unwrap()
}

fn anime_library() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let anime = root.path().join("Frieren");
    let cour = anime.join("Cour 01");
    fs::create_dir_all(&cour).unwrap();
    fs::write(
        anime.join("anime.nfo"),
        r#"<anime>
            <title>葬送のフリーレン</title>
            <plot>旅の終わりから始まる物語。</plot>
            <relation>adaptation</relation>
        </anime>"#,
    )
    .unwrap();
    fs::write(
        cour.join("cour.nfo"),
        "<courdetails><cour>1</cour></courdetails>",
    )
    .unwrap();
    fs::write(
        cour.join("C01E001.nfo"),
        r#"<episodedetails>
            <title>冒険の終わり</title>
            <cour>1</cour>
            <episodeclass>regular</episodeclass>
            <airednumber>1</airednumber>
            <absolutenumber>1</absolutenumber>
        </episodedetails>"#,
    )
    .unwrap();
    root
}

#[test]
fn anime_search_and_resolve_work_offline() {
    let root = anime_library();
    let search = run(fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args(["search", "anime", "葬送のフリーレン", "--year", "2023"]));
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let stdout = String::from_utf8(search.stdout).unwrap();
    assert!(stdout.contains("葬送のフリーレン"));
    assert!(stdout.contains("local"));

    let resolve = run(fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "resolve",
            "anime",
            "葬送のフリーレン",
            "--year",
            "2023",
            "--json",
        ]));
    assert_eq!(
        resolve.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "anime");
    assert_eq!(value["title"], "葬送のフリーレン");
    assert_eq!(value["relation"], "adaptation");
    assert_eq!(value["cours"], 1);
    assert_eq!(value["episodes"], 1);
    assert_eq!(value["titles"].as_array().unwrap().len(), 1);
    assert!(
        value["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning.as_str().unwrap().contains("offline mode skipped"))
    );
}
