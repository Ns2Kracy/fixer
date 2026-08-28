use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/library")
        .join(name)
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                visit(root, &entry.path(), files);
            } else {
                files.insert(
                    entry.path().strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn offline_movie_nfo_produces_a_dry_run_plan_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("movie");
    copy_tree(&fixture("movie"), &library);
    let input = library.join("In the Mood for Love (2000)");
    let before = snapshot(&library);

    let output = fixer()
        .arg("--offline")
        .arg("plan")
        .arg(&input)
        .args(["--kind", "movie", "--json"])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["schema_version"], 1);
    assert_eq!(plan["kind"], "movie");
    assert_eq!(plan["operations"].as_array().unwrap().len(), 1);
    assert_eq!(plan["operations"][0]["operation"], "write_bytes");
    assert_eq!(plan["operations"][0]["target"], "movie.json");
    assert!(plan["operations"][0].get("content").is_none());
    assert_eq!(snapshot(&library), before);
    assert!(!input.join("movie.json").exists());
}

#[test]
fn ambiguous_anime_candidates_are_visible_and_cannot_trigger_a_broad_write() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("anime");
    copy_tree(&fixture("anime"), &library);
    let before = snapshot(&library);

    let search = fixer()
        .arg("--local-root")
        .arg(&library)
        .arg("--offline")
        .args(["search", "anime", "Fixture Journey"])
        .output()
        .unwrap();
    assert_eq!(search.status.code(), Some(3));
    let stdout = String::from_utf8(search.stdout).unwrap();
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.contains("Fixture Journey"))
            .count(),
        2
    );
    assert!(
        String::from_utf8_lossy(&search.stderr).contains("ambiguous_candidates")
    );

    let scrape = fixer()
        .arg("--offline")
        .arg("scrape")
        .arg(&library)
        .args(["--kind", "anime", "--dry-run"])
        .output()
        .unwrap();
    assert_eq!(scrape.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&scrape.stderr)
            .contains("ambiguous anime input: found 2 series")
    );
    assert_eq!(snapshot(&library), before);
}

#[test]
fn music_release_preserves_two_disc_and_track_identities() {
    let directory = tempfile::tempdir().unwrap();
    let library = directory.path().join("music");
    copy_tree(&fixture("music"), &library);

    let output = fixer()
        .arg("--local-root")
        .arg(&library)
        .arg("--offline")
        .args(["resolve", "music", "Kind of Blue", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let discs = value["releases"][0]["discs"].as_array().unwrap();
    assert_eq!(discs.len(), 2);
    assert_eq!(discs[0]["number"], 1);
    assert_eq!(discs[0]["tracks"][0]["disc"], 1);
    assert_eq!(discs[0]["tracks"][0]["track"], 1);
    assert_eq!(discs[0]["tracks"][0]["title"], "So What");
    assert_eq!(discs[1]["number"], 2);
    assert_eq!(discs[1]["tracks"][0]["disc"], 2);
    assert_eq!(discs[1]["tracks"][0]["track"], 1);
    assert_eq!(discs[1]["tracks"][0]["title"], "Alternate Take");
}
