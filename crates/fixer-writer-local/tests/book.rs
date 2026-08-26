use fixer_core::{
    Asset, AssetId, AssetKind, BookEdition, BookWork, Credit, CreditRole, Isbn10, Isbn13,
    LocalizedValue, OutputOperation, Person, PersonId, ProvenanceMap, ReleaseId, Resolved,
    SourcePath, WorkId,
};
use fixer_writer_local::BookWriter;
use std::{collections::BTreeMap, path::PathBuf};

fn resolved_book() -> Resolved<BookWork> {
    let mut titles = LocalizedValue::new();
    titles
        .insert("und", "The Left Hand of Darkness".to_owned())
        .unwrap();
    let author = Person::new(
        PersonId::new("author-le-guin").unwrap(),
        "Ursula K. Le Guin",
    )
    .unwrap();
    let local_cover = Asset::new(
        AssetId::new("local-cover").unwrap(),
        SourcePath::new("/library/covers/left-hand.png").unwrap(),
        AssetKind::Artwork,
    );
    let editions = vec![
        BookEdition::new(
            ReleaseId::new("edition-ace").unwrap(),
            Isbn10::new("0441478123").unwrap(),
            Isbn13::new("9780441478125").unwrap(),
            "Ace Books",
            vec![local_cover],
        )
        .unwrap(),
        BookEdition::new(
            ReleaseId::new("edition-orbit").unwrap(),
            Isbn10::new("1473225949").unwrap(),
            Isbn13::new("9781473225947").unwrap(),
            "Orbit",
            Vec::new(),
        )
        .unwrap(),
    ];
    Resolved {
        value: BookWork::new(
            WorkId::new("work-left-hand-darkness").unwrap(),
            titles,
            vec![Credit::new(author, CreditRole::Author)],
            editions,
        ),
        provenance: ProvenanceMap::new(),
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings: Vec::new(),
    }
}

fn writes(plan: &fixer_core::OutputPlan) -> BTreeMap<PathBuf, Vec<u8>> {
    plan.operations()
        .iter()
        .filter_map(|operation| match operation {
            OutputOperation::WriteBytes { target, content } => {
                Some((target.clone(), content.as_bytes().to_vec()))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn exact_edition_plans_deterministic_opf_json_manifest_and_cover_bytes() {
    let root = tempfile::tempdir().unwrap();
    let target = root.path().join("not-created");
    let writer = BookWriter::for_isbn(Isbn13::new("9781473225947").unwrap())
        .with_cover_bytes("jpg", b"cover-bytes".to_vec())
        .unwrap();

    let first = writer.plan_resolved(&resolved_book(), &target).unwrap();
    let second = writer.plan_resolved(&resolved_book(), &target).unwrap();

    assert_eq!(first, second);
    assert!(!target.exists());
    let files = writes(&first);
    assert_eq!(files[&PathBuf::from("cover.jpg")], b"cover-bytes");
    let opf = String::from_utf8(files[&PathBuf::from("book.opf")].clone()).unwrap();
    assert!(opf.contains("9781473225947"));
    assert!(opf.contains("Orbit"));
    assert!(opf.contains("Ursula K. Le Guin"));
    assert!(!opf.contains("9780441478125"));
    let manifest = String::from_utf8(files[&PathBuf::from("fixer-manifest.json")].clone()).unwrap();
    assert!(manifest.contains("book.opf"));
    assert!(manifest.contains("book.json"));
    assert!(manifest.contains("cover.jpg"));
}

#[test]
fn local_cover_is_a_planned_copy_and_epub_mutation_is_only_a_confirmation_intent() {
    let writer = BookWriter::for_isbn(Isbn13::new("9780441478125").unwrap())
        .with_epub_mutation_target("/library/The Left Hand of Darkness.epub");
    let plan = writer.plan_resolved(&resolved_book(), "output").unwrap();

    assert!(plan.operations().iter().any(|operation| matches!(
        operation,
        OutputOperation::Copy { source, target }
            if source.as_path() == std::path::Path::new("/library/covers/left-hand.png")
                && target.as_path() == std::path::Path::new("cover.png")
    )));
    assert!(!plan.operations().iter().any(|operation| {
        operation
            .target()
            .is_some_and(|target| target.extension().is_some_and(|ext| ext == "epub"))
    }));
    let files = writes(&plan);
    let intent =
        String::from_utf8(files[&PathBuf::from("epub-mutation-intent.json")].clone()).unwrap();
    assert!(intent.contains("\"requires_confirmation\": true"));
    assert!(intent.contains("The Left Hand of Darkness.epub"));
}

#[test]
fn remote_cover_is_declared_for_acquisition_instead_of_copied_as_a_path() {
    let mut resolved = resolved_book();
    resolved.value.editions[1].assets.push(Asset::new(
        AssetId::new("remote-cover").unwrap(),
        SourcePath::new("https://covers.openlibrary.org/b/id/123-L.jpg").unwrap(),
        AssetKind::Artwork,
    ));

    let plan = BookWriter::for_isbn(Isbn13::new("9781473225947").unwrap())
        .plan_resolved(&resolved, "output")
        .unwrap();

    assert!(!plan.operations().iter().any(|operation| matches!(
        operation,
        OutputOperation::Copy { source, .. }
            if source.to_string_lossy().starts_with("https://")
    )));
    let intent =
        String::from_utf8(writes(&plan)[&PathBuf::from("cover-acquisition-intent.json")].clone())
            .unwrap();
    assert!(intent.contains("\"requires_network\": true"));
    assert!(intent.contains("https://covers.openlibrary.org/b/id/123-L.jpg"));
    assert!(intent.contains("cover.jpg"));
}

#[test]
fn unknown_edition_is_rejected_instead_of_falling_back_to_the_first() {
    let error = BookWriter::for_isbn(Isbn13::new("9780061054884").unwrap())
        .plan_resolved(&resolved_book(), "output")
        .unwrap_err();

    assert!(error.to_string().contains("9780061054884"));
    assert!(error.to_string().contains("not present"));
}
