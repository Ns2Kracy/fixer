use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, HttpError, HttpRequest, HttpResponse,
    MediaKind, MetadataDocument, Provider, SearchRequest,
};
use fixer_provider_local::{LocalProvider, scan_books};
use std::{
    fs,
    io::{Cursor, Write},
    path::Path,
};
use tempfile::tempdir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

#[derive(Debug)]
struct NoHttp;

impl HttpClient for NoHttp {
    fn execute<'a>(&'a self, _: HttpRequest) -> BoxFuture<'a, Result<HttpResponse, HttpError>> {
        Box::pin(async { panic!("local book provider must not use HTTP") })
    }
}

fn write_epub(path: &Path, isbn: &str, publisher: &str) {
    let opf = format!(
        r#"<package xmlns:dc="http://purl.org/dc/elements/1.1/"><metadata>
<dc:identifier>urn:isbn:{isbn}</dc:identifier>
<dc:title>The Left Hand of Darkness</dc:title>
<dc:creator>Ursula K. Le Guin</dc:creator>
<dc:publisher>{publisher}</dc:publisher>
</metadata></package>"#
    );
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
    fs::write(path, archive.finish().unwrap().into_inner()).unwrap();
}

#[test]
fn matching_epubs_form_one_work_with_distinct_editions_and_source_assets() {
    let root = tempdir().unwrap();
    let books = root
        .path()
        .join("Ursula K. Le Guin/The Left Hand of Darkness");
    fs::create_dir_all(&books).unwrap();
    write_epub(&books.join("ace.epub"), "9780441478125", "Ace Books");
    write_epub(&books.join("orbit.epub"), "9781473225947", "Orbit");

    let result = scan_books(root.path()).unwrap();

    assert!(result.warnings.is_empty());
    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.roots, vec![books.clone()]);
    let work = &result.documents[0];
    assert_eq!(
        work.titles.entries()[0].value(),
        "The Left Hand of Darkness"
    );
    assert_eq!(work.contributors.len(), 1);
    assert_eq!(work.contributors[0].person.name, "Ursula K. Le Guin");
    assert_eq!(work.editions.len(), 2);
    assert_eq!(work.editions[0].isbn_13.as_str(), "9780441478125");
    assert_eq!(work.editions[1].isbn_13.as_str(), "9781473225947");
    assert_eq!(work.editions[0].assets.len(), 1);
    assert_eq!(
        work.editions[0].assets[0].source_path.as_str(),
        books.join("ace.epub").to_string_lossy()
    );
    assert_ne!(work.id.as_str(), work.editions[0].id.as_str());
}

#[test]
fn malformed_epubs_become_path_warnings_without_hiding_valid_books() {
    let root = tempdir().unwrap();
    write_epub(
        &root.path().join("valid.epub"),
        "9780441478125",
        "Ace Books",
    );
    fs::write(root.path().join("broken.epub"), b"not a zip").unwrap();

    let result = scan_books(root.path()).unwrap();

    assert_eq!(result.documents.len(), 1);
    assert_eq!(result.warnings.len(), 1);
    assert!(result.warnings[0].path.ends_with("broken.epub"));
}

#[test]
fn scanned_book_is_registered_with_exact_isbn_identity_and_no_http() {
    let root = tempdir().unwrap();
    write_epub(&root.path().join("book.epub"), "9780441478125", "Ace Books");

    let (provider, warnings) = LocalProvider::from_scan(root.path()).unwrap();
    assert!(warnings.is_empty());
    assert!(!provider.descriptor().requires_network());
    let candidates = futures_lite::future::block_on(provider.search(
        SearchRequest::book("The Left Hand of Darkness", None).unwrap(),
        &NoHttp,
    ))
    .unwrap();
    let Candidate::Book(candidate) = &candidates[0] else {
        panic!("expected book candidate");
    };
    assert_eq!(candidate.external_id.namespace, "isbn");
    assert_eq!(candidate.external_id.value, "9780441478125");

    let document = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(MediaKind::Book, candidate.external_id.clone()),
        &NoHttp,
    ))
    .unwrap();
    assert!(matches!(document, MetadataDocument::Book(_)));
}

#[test]
fn pre_parsed_book_provider_registers_every_edition_isbn() {
    let root = tempdir().unwrap();
    write_epub(&root.path().join("ace.epub"), "9780441478125", "Ace Books");
    write_epub(&root.path().join("orbit.epub"), "9781473225947", "Orbit");
    let work = scan_books(root.path()).unwrap().documents.remove(0);
    let provider = LocalProvider::from_book_documents([work]).unwrap();

    let document = futures_lite::future::block_on(provider.fetch(
        FetchRequest::new(
            MediaKind::Book,
            fixer_core::ExternalId::new("isbn", "9781473225947").unwrap(),
        ),
        &NoHttp,
    ))
    .unwrap();

    assert!(matches!(document, MetadataDocument::Book(_)));
}

#[cfg(unix)]
#[test]
fn book_scan_does_not_follow_symlinked_directories() {
    use std::os::unix::fs::symlink;

    let root = tempdir().unwrap();
    let outside = tempdir().unwrap();
    write_epub(
        &outside.path().join("outside.epub"),
        "9780441478125",
        "Ace Books",
    );
    symlink(outside.path(), root.path().join("linked-books")).unwrap();

    let result = scan_books(root.path()).unwrap();
    assert!(result.documents.is_empty());
}
