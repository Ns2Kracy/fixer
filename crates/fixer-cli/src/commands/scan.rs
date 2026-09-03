use crate::{
    AppError, AppResult, RunStatus,
    args::{MediaKindArg, ScanArgs},
    json::ScanDto,
    render,
};
use fixer_provider_local::{scan, scan_anime, scan_books, scan_music, scan_television};
use std::path::{Path, PathBuf};

pub fn run(args: &ScanArgs) -> AppResult<RunStatus> {
    if !args.path.exists() {
        return Err(AppError::invalid_input(format!(
            "input path does not exist: {}",
            args.path.display()
        )));
    }
    let root = canonical_scan_root(&args.path)?;
    let (kind, documents, warnings) = match args.kind {
        MediaKindArg::Anime => {
            let result = scan_anime(&root).map_err(AppError::new)?;
            ("anime", result.documents.len(), result.warnings)
        }
        MediaKindArg::Book => {
            let result = scan_books(&root).map_err(AppError::new)?;
            ("book", result.documents.len(), result.warnings)
        }
        MediaKindArg::Movie => {
            let result = scan(&root).map_err(AppError::new)?;
            ("movie", result.documents.len(), result.warnings)
        }
        MediaKindArg::Music => {
            let result = scan_music(&root).map_err(AppError::new)?;
            ("music", result.documents.len(), result.warnings)
        }
        MediaKindArg::Television => {
            let result = scan_television(&root).map_err(AppError::new)?;
            ("television", result.documents.len(), result.warnings)
        }
    };

    if args.json {
        render::json(&ScanDto::new(kind, &root, documents, &warnings))?;
    } else {
        println!(
            "scanned {documents} {kind} document(s) at {}",
            root.display()
        );
    }
    Ok(super::finish_with_warnings(&warnings))
}

fn canonical_scan_root(path: &Path) -> AppResult<PathBuf> {
    let root = if path.is_dir() {
        path
    } else {
        path.parent()
            .ok_or_else(|| AppError::invalid_input("input path has no parent directory"))?
    };
    root.canonicalize().map_err(AppError::new)
}
