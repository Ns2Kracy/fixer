use fixer_provider_local::{parse_epub, parse_opf};
use std::io::{Cursor, Write};
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

const OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf"
         xmlns:dc="http://purl.org/dc/elements/1.1/"
         version="3.0" unique-identifier="book-id">
  <metadata>
    <dc:identifier id="book-id">urn:isbn:978-0-441-47812-5</dc:identifier>
    <dc:title>The Left Hand of Darkness</dc:title>
    <dc:creator>Ursula K. Le Guin</dc:creator>
    <dc:creator>Le Guin, Ursula K.</dc:creator>
    <dc:publisher>Ace Books</dc:publisher>
  </metadata>
</package>"#;

#[test]
fn opf_extracts_title_authors_publisher_and_canonical_isbns() {
    let metadata = parse_opf(OPF).unwrap();

    assert_eq!(metadata.title, "The Left Hand of Darkness");
    assert_eq!(
        metadata.authors,
        vec!["Ursula K. Le Guin", "Le Guin, Ursula K."]
    );
    assert_eq!(metadata.publisher.as_deref(), Some("Ace Books"));
    assert_eq!(metadata.isbn_10.as_ref().unwrap().as_str(), "0441478123");
    assert_eq!(metadata.isbn_13.as_ref().unwrap().as_str(), "9780441478125");
}

#[test]
fn opf_rejects_missing_title_author_or_valid_isbn() {
    assert!(parse_opf("<package><metadata/></package>").is_err());
    assert!(parse_opf(
        "<package><metadata><title>Book</title><creator>Author</creator><identifier>not-an-isbn</identifier></metadata></package>"
    )
    .is_err());
}

#[test]
fn epub_resolves_container_rootfile_and_does_not_modify_input_bytes() {
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
            br#"<?xml version="1.0"?>
<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0">
  <rootfiles>
    <rootfile full-path="OPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
        )
        .unwrap();
    archive.start_file("OPS/content.opf", deflated).unwrap();
    archive.write_all(OPF.as_bytes()).unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    let original = bytes.clone();

    let metadata = parse_epub(&bytes).unwrap();

    assert_eq!(metadata.title, "The Left Hand of Darkness");
    assert_eq!(metadata.isbn_13.unwrap().as_str(), "9780441478125");
    assert_eq!(bytes, original);
}

#[test]
fn epub_rejects_missing_container_or_unsafe_rootfile_paths() {
    let empty = ZipWriter::new(Cursor::new(Vec::new()))
        .finish()
        .unwrap()
        .into_inner();
    assert!(parse_epub(&empty).is_err());

    let mut archive = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    archive
        .start_file("META-INF/container.xml", options)
        .unwrap();
    archive
        .write_all(
            br#"<container><rootfiles><rootfile full-path="../content.opf"/></rootfiles></container>"#,
        )
        .unwrap();
    let bytes = archive.finish().unwrap().into_inner();
    assert!(parse_epub(&bytes).is_err());
}
