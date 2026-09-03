//! Deterministic book metadata and confirmation-gated EPUB update planning.

use fixer_core::{
    Asset, AssetKind, BookEdition, BookWork, Isbn13, MetadataDocument, OutputOperation, OutputPlan,
    PlannedContent, PlanningError, ProvenanceMap, Resolved, WriteRequest, Writer,
};
use serde::Serialize;
use std::fmt::Write as _;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const EPUB_MUTATION_WARNING: &str = "Updating metadata inside an EPUB rewrites the archive; execute only after explicit confirmation and preserve the original book.";

/// Plans metadata for one explicitly selected book edition without performing I/O.
#[derive(Debug, Clone)]
pub struct BookWriter {
    isbn: Isbn13,
    cover: Option<CoverBytes>,
    epub_mutation_target: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct CoverBytes {
    extension: String,
    bytes: Vec<u8>,
}

impl BookWriter {
    /// Selects the exact edition to write by ISBN-13.
    pub const fn for_isbn(isbn: Isbn13) -> Self {
        Self {
            isbn,
            cover: None,
            epub_mutation_target: None,
        }
    }

    /// Supplies already-acquired cover bytes. No network request is performed by this writer.
    pub fn with_cover_bytes(
        mut self,
        extension: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Result<Self, PlanningError> {
        let extension = normalized_extension(&extension.into()).ok_or_else(|| {
            PlanningError::InvalidPlan(
                "cover extension must contain only ASCII letters or digits".to_owned(),
            )
        })?;
        if bytes.is_empty() {
            return Err(PlanningError::InvalidPlan(
                "cover bytes must not be empty".to_owned(),
            ));
        }
        self.cover = Some(CoverBytes { extension, bytes });
        Ok(self)
    }

    /// Declares an EPUB that may be updated after a separate confirmation step.
    ///
    /// The resulting plan never targets the EPUB itself; it writes an inspectable intent file.
    pub fn with_epub_mutation_target(mut self, target: impl Into<PathBuf>) -> Self {
        self.epub_mutation_target = Some(target.into());
        self
    }

    /// Plans output for the selected edition and carries resolved provenance into the manifest.
    pub fn plan_resolved(
        &self,
        resolved: &Resolved<BookWork>,
        output_root: impl AsRef<Path>,
    ) -> Result<OutputPlan, PlanningError> {
        self.plan_book(
            &resolved.value,
            Some(&resolved.provenance),
            output_root.as_ref(),
        )
    }

    fn plan_book(
        &self,
        work: &BookWork,
        provenance: Option<&ProvenanceMap>,
        output_root: &Path,
    ) -> Result<OutputPlan, PlanningError> {
        let edition = work
            .editions
            .iter()
            .find(|edition| edition.isbn_13 == self.isbn)
            .ok_or_else(|| {
                PlanningError::InvalidPlan(format!(
                    "ISBN-13 `{}` is not present in book work `{}`",
                    self.isbn.as_str(),
                    work.id.as_str()
                ))
            })?;
        let title = first_title(work)?;
        let authors = work
            .contributors
            .iter()
            .filter(|credit| credit.role == fixer_core::CreditRole::Author)
            .map(|credit| credit.person.name.as_str())
            .collect::<Vec<_>>();

        let mut plan = OutputPlan::new(output_root);
        plan.push(write_bytes(
            "book.opf",
            opf(title, &authors, edition).into_bytes(),
        )?);
        plan.push(write_json(
            "book.json",
            &BookDocument {
                work,
                selected_edition: edition,
            },
        )?);
        let mut planned_files = BTreeSet::from(["book.json".to_owned(), "book.opf".to_owned()]);

        if let Some(cover) = &self.cover {
            let target = format!("cover.{}", cover.extension);
            plan.push(write_bytes(target.clone(), cover.bytes.clone())?);
            planned_files.insert(target);
        } else if let Some(asset) = edition
            .assets
            .iter()
            .find(|asset| asset.kind == AssetKind::Artwork)
        {
            plan_cover(asset, &mut plan, &mut planned_files)?;
        }

        if let Some(epub_path) = &self.epub_mutation_target {
            if epub_path.as_os_str().is_empty() {
                return Err(PlanningError::InvalidPlan(
                    "EPUB mutation target must not be empty".to_owned(),
                ));
            }
            let intent = EpubMutationIntent {
                schema_version: 1,
                requires_confirmation: true,
                warning: EPUB_MUTATION_WARNING,
                epub_path,
                metadata_source: "book.opf",
            };
            plan.push(write_json("epub-mutation-intent.json", &intent)?);
            planned_files.insert("epub-mutation-intent.json".to_owned());
        }

        let manifest = BookManifest {
            schema_version: 1,
            work_id: work.id.as_str(),
            edition_id: edition.id.as_str(),
            isbn_13: edition.isbn_13.as_str(),
            provenance,
            planned_files,
        };
        plan.push(write_json("fixer-manifest.json", &manifest)?);
        Ok(plan)
    }
}

impl Writer for BookWriter {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError> {
        match request.document {
            MetadataDocument::Book(work) => {
                self.plan_book(&work, None, request.output_root.as_path())
            }
            _ => Err(PlanningError::UnsupportedDocument),
        }
    }
}

#[derive(Serialize)]
struct BookDocument<'a> {
    work: &'a BookWork,
    selected_edition: &'a BookEdition,
}

#[derive(Serialize)]
struct BookManifest<'a> {
    schema_version: u8,
    work_id: &'a str,
    edition_id: &'a str,
    isbn_13: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    provenance: Option<&'a ProvenanceMap>,
    planned_files: BTreeSet<String>,
}

#[derive(Serialize)]
struct CoverAcquisitionIntent<'a> {
    schema_version: u8,
    requires_network: bool,
    source: &'a str,
    target: String,
}

#[derive(Serialize)]
struct EpubMutationIntent<'a> {
    schema_version: u8,
    requires_confirmation: bool,
    warning: &'static str,
    epub_path: &'a Path,
    metadata_source: &'static str,
}

fn plan_cover(
    asset: &Asset,
    plan: &mut OutputPlan,
    planned_files: &mut BTreeSet<String>,
) -> Result<(), PlanningError> {
    let source = asset.source_path.as_str();
    let extension = extension(source).unwrap_or_else(|| "jpg".to_owned());
    let target = format!("cover.{extension}");
    if source.starts_with("https://") || source.starts_with("http://") {
        let intent = CoverAcquisitionIntent {
            schema_version: 1,
            requires_network: true,
            source,
            target,
        };
        plan.push(write_json("cover-acquisition-intent.json", &intent)?);
        planned_files.insert("cover-acquisition-intent.json".to_owned());
    } else {
        plan.push(OutputOperation::copy(source, &target)?);
        planned_files.insert(target);
    }
    Ok(())
}

fn opf(title: &str, authors: &[&str], edition: &BookEdition) -> String {
    let mut creators = String::new();
    for author in authors {
        let _ = writeln!(
            creators,
            "    <dc:creator>{}</dc:creator>",
            escape_xml(author)
        );
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" version=\"3.0\">\n  <metadata>\n    <dc:identifier>urn:isbn:{}</dc:identifier>\n    <dc:title>{}</dc:title>\n{}    <dc:publisher>{}</dc:publisher>\n  </metadata>\n</package>\n",
        edition.isbn_13.as_str(),
        escape_xml(title),
        creators,
        escape_xml(&edition.publisher),
    )
}

fn first_title(work: &BookWork) -> Result<&str, PlanningError> {
    work.titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .ok_or_else(|| PlanningError::InvalidPlan("book work has no title".to_owned()))
}

fn extension(source: &str) -> Option<String> {
    let without_query = source.split(['?', '#']).next().unwrap_or(source);
    normalized_extension(without_query.rsplit_once('.')?.1)
}

fn normalized_extension(value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('.');
    (!value.is_empty() && value.len() <= 8 && value.chars().all(|ch| ch.is_ascii_alphanumeric()))
        .then(|| value.to_ascii_lowercase())
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn write_json(
    target: impl Into<PathBuf>,
    value: &impl Serialize,
) -> Result<OutputOperation, PlanningError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    write_bytes(target, bytes)
}

fn write_bytes(
    target: impl Into<PathBuf>,
    bytes: Vec<u8>,
) -> Result<OutputOperation, PlanningError> {
    Ok(OutputOperation::write_bytes(
        target,
        PlannedContent::new(bytes),
    )?)
}
