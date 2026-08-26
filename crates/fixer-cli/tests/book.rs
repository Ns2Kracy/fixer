use std::{
    fs,
    io::{Cursor, Write},
    process::Command,
};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn fixer() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fixer"))
}

fn book_library() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let directory = root
        .path()
        .join("Ursula K. Le Guin/The Left Hand of Darkness");
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
    fs::write(
        directory.join("book.epub"),
        archive.finish().unwrap().into_inner(),
    )
    .unwrap();
    root
}

#[test]
fn book_commands_expose_title_year_isbn_and_json_options() {
    for command in ["search", "resolve"] {
        let output = fixer().args([command, "book", "--help"]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("--year"));
        assert!(stdout.contains("--isbn"));
        if command == "resolve" {
            assert!(stdout.contains("--json"));
        }
    }
}

#[test]
fn book_search_and_exact_isbn_resolve_work_offline() {
    let root = book_library();
    let search = fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "search",
            "book",
            "The Left Hand of Darkness",
            "--isbn",
            "9780441478125",
        ])
        .output()
        .unwrap();
    assert!(
        search.status.success(),
        "{}",
        String::from_utf8_lossy(&search.stderr)
    );
    let stdout = String::from_utf8(search.stdout).unwrap();
    assert!(stdout.contains("The Left Hand of Darkness"));
    assert!(stdout.contains("local"));
    assert!(stdout.contains("9780441478125"));

    let resolve = fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "resolve",
            "book",
            "The Left Hand of Darkness",
            "--year",
            "1969",
            "--isbn",
            "9780441478125",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        resolve.status.success(),
        "{}",
        String::from_utf8_lossy(&resolve.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&resolve.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "book");
    assert_eq!(value["title"], "The Left Hand of Darkness");
    assert_eq!(value["contributors"][0]["name"], "Ursula K. Le Guin");
    assert_eq!(value["contributors"][0]["role"], "author");
    assert_eq!(value["editions"].as_array().unwrap().len(), 1);
    assert_eq!(value["editions"][0]["isbn_10"], "0441478123");
    assert_eq!(value["editions"][0]["isbn_13"], "9780441478125");
    assert_eq!(value["editions"][0]["publisher"], "Ace Books");
}

#[test]
fn invalid_book_isbn_is_rejected_before_provider_search() {
    let root = book_library();
    let output = fixer()
        .arg("--local-root")
        .arg(root.path())
        .arg("--offline")
        .args([
            "resolve",
            "book",
            "The Left Hand of Darkness",
            "--isbn",
            "not-an-isbn",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("isbn_13")
    );
}
