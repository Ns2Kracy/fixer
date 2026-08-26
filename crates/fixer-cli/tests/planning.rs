use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Directory,
    File(Vec<u8>),
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        let mut children = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        children.sort();
        for child in children {
            let relative = child.strip_prefix(root).unwrap().to_path_buf();
            if child.is_dir() {
                entries.insert(relative, Entry::Directory);
                visit(root, &child, entries);
            } else {
                entries.insert(relative, Entry::File(fs::read(child).unwrap()));
            }
        }
    }

    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

fn anime_fixture(root: &Path) -> PathBuf {
    let anime = root.join("Frieren");
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
    anime
}

fn book_fixture(root: &Path) -> PathBuf {
    let directory = root.join("Ursula K. Le Guin/The Left Hand of Darkness");
    fs::create_dir_all(&directory).unwrap();
    let opf = r#"<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata>
<dc:identifier>urn:isbn:9780441478125</dc:identifier>
<dc:title>The Left Hand of Darkness</dc:title>
<dc:creator>Ursula K. Le Guin</dc:creator>
<dc:publisher>Ace Books</dc:publisher>
</metadata></package>"#;
    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("mimetype", stored).unwrap();
    archive.write_all(b"application/epub+zip").unwrap();
    archive
        .start_file("META-INF/container.xml", deflated)
        .unwrap();
    archive
        .write_all(
            br#"<container><rootfiles><rootfile full-path="OPS/content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
    archive.start_file("OPS/content.opf", deflated).unwrap();
    archive.write_all(opf.as_bytes()).unwrap();
    let path = directory.join("book.epub");
    fs::write(&path, archive.finish().unwrap().into_inner()).unwrap();
    path
}

fn movie_fixture(root: &Path) -> PathBuf {
    fs::write(
        root.join("movie-source.json"),
        include_str!("../../fixer-provider-local/tests/fixtures/movie.json"),
    )
    .unwrap();
    root.to_path_buf()
}

fn music_fixture(root: &Path) -> PathBuf {
    let album = root.join("Miles Davis/Kind of Blue");
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
    album
}

fn television_fixture(root: &Path) -> PathBuf {
    let season = root.join("Example Show/Season 01");
    fs::create_dir_all(&season).unwrap();
    fs::write(season.join("Example.Show.S01E02.mkv"), b"episode").unwrap();
    root.join("Example Show")
}

type FixtureBuilder = fn(&Path) -> PathBuf;
type FixtureCase = (&'static str, FixtureBuilder);

fn json_keys(value: &serde_json::Value) -> BTreeSet<&str> {
    value
        .as_object()
        .expect("JSON contract value must be an object")
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn plan_dispatches_all_media_with_stable_json_and_never_writes() {
    let fixtures: [FixtureCase; 5] = [
        ("anime", anime_fixture),
        ("book", book_fixture),
        ("movie", movie_fixture),
        ("music", music_fixture),
        ("television", television_fixture),
    ];

    for (kind, build_fixture) in fixtures {
        let root = tempfile::tempdir().unwrap();
        let input = build_fixture(root.path());
        let before = snapshot(root.path());
        let output = fixer()
            .arg("--offline")
            .arg("plan")
            .arg(&input)
            .args(["--kind", kind, "--json"])
            .output()
            .unwrap();

        assert_eq!(
            output.status.code(),
            Some(3),
            "{kind}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            json_keys(&value),
            BTreeSet::from(["kind", "operations", "output_root", "schema_version"]),
            "{kind}"
        );
        assert_eq!(value["schema_version"], 1, "{kind}");
        assert_eq!(value["kind"], kind, "{kind}");
        assert!(value["output_root"].is_string(), "{kind}");
        let operations = value["operations"].as_array().unwrap();
        assert!(!operations.is_empty(), "{kind}");
        for operation in operations {
            let keys = json_keys(operation);
            assert!(
                keys == BTreeSet::from(["operation", "target"])
                    || keys == BTreeSet::from(["operation", "source", "target"]),
                "{kind}: {operation}"
            );
            assert!(operation["operation"].is_string(), "{kind}");
            assert!(operation["target"].is_string(), "{kind}");
            assert!(operation.get("content").is_none(), "{kind}");
            assert!(operation.get("bytes").is_none(), "{kind}");
        }
        assert_eq!(snapshot(root.path()), before, "plan mutated {kind} fixture");
    }
}
