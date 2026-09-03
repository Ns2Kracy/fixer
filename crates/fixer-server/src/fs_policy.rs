use std::{
    io,
    path::{Path, PathBuf},
};

use fixer_core::{OutputOperation, OutputPlan};
use thiserror::Error;

/// Canonical filesystem roots under which server media access is permitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsPolicy {
    roots: Vec<PathBuf>,
}

impl FsPolicy {
    pub fn new<I, P>(roots: I) -> Result<Self, FsPolicyError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut canonical = roots
            .into_iter()
            .map(|root| canonical_directory(root.as_ref()))
            .collect::<Result<Vec<_>, _>>()?;
        canonical.sort();
        canonical.dedup();
        if canonical.is_empty() {
            return Err(FsPolicyError::NoRoots);
        }
        Ok(Self { roots: canonical })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Validates and canonicalizes an existing readable path.
    pub fn validate_read(&self, path: impl AsRef<Path>) -> Result<PathBuf, FsPolicyError> {
        let path = path.as_ref();
        let canonical = path
            .canonicalize()
            .map_err(|source| FsPolicyError::Canonicalize {
                path: path.to_owned(),
                source,
            })?;
        self.require_allowed(path, &canonical)?;
        Ok(canonical)
    }

    /// Validates a potentially non-existent writable path using its nearest
    /// existing ancestor, preventing traversal through symlinks outside a root.
    pub fn validate_write(&self, path: impl AsRef<Path>) -> Result<PathBuf, FsPolicyError> {
        let path = absolute(path.as_ref())?;
        let ancestor = nearest_existing_ancestor(&path)?;
        let canonical = ancestor
            .canonicalize()
            .map_err(|source| FsPolicyError::Canonicalize {
                path: ancestor.to_owned(),
                source,
            })?;
        self.require_allowed(&path, &canonical)?;
        Ok(path)
    }

    /// Revalidates all paths in an output plan immediately before execution.
    ///
    /// # Panics
    ///
    /// Panics if the plan contains an operation without a target, which violates
    /// the `OutputOperation` invariant.
    pub fn validate_plan(&self, plan: &OutputPlan) -> Result<(), FsPolicyError> {
        let root = self.validate_write(&plan.output_root)?;
        for operation in plan.operations() {
            let target = operation
                .target()
                .expect("all output operations have a target");
            let target = if target.is_absolute() {
                target.to_owned()
            } else {
                root.join(target)
            };
            self.validate_write(target)?;

            if let Some(source) = operation.source() {
                let source = resolve_source(operation, &root, source);
                self.validate_read(source)?;
            }
        }
        Ok(())
    }

    fn require_allowed(&self, requested: &Path, canonical: &Path) -> Result<(), FsPolicyError> {
        if self.roots.iter().any(|root| canonical.starts_with(root)) {
            Ok(())
        } else {
            Err(FsPolicyError::OutsideAllowedRoots {
                path: requested.to_owned(),
            })
        }
    }
}

fn resolve_source(operation: &OutputOperation, root: &Path, source: &Path) -> PathBuf {
    if source.is_absolute() {
        return source.to_owned();
    }
    if matches!(operation, OutputOperation::Symlink { .. }) {
        let target = operation
            .target()
            .expect("all output operations have a target");
        return root.join(target).parent().unwrap_or(root).join(source);
    }
    root.join(source)
}

fn canonical_directory(path: &Path) -> Result<PathBuf, FsPolicyError> {
    let canonical = path
        .canonicalize()
        .map_err(|source| FsPolicyError::Canonicalize {
            path: path.to_owned(),
            source,
        })?;
    if !canonical.is_dir() {
        return Err(FsPolicyError::RootNotDirectory {
            path: path.to_owned(),
        });
    }
    Ok(canonical)
}

fn absolute(path: &Path) -> Result<PathBuf, FsPolicyError> {
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        std::env::current_dir()
            .map(|current| current.join(path))
            .map_err(|source| FsPolicyError::CurrentDirectory { source })
    }
}

fn nearest_existing_ancestor(path: &Path) -> Result<&Path, FsPolicyError> {
    let mut candidate = path;
    loop {
        match std::fs::symlink_metadata(candidate) {
            Ok(_) => return Ok(candidate),
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                candidate =
                    candidate
                        .parent()
                        .ok_or_else(|| FsPolicyError::NoExistingAncestor {
                            path: path.to_owned(),
                        })?;
            }
            Err(source) => {
                return Err(FsPolicyError::Inspect {
                    path: candidate.to_owned(),
                    source,
                });
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum FsPolicyError {
    #[error("at least one allowed media root is required")]
    NoRoots,
    #[error("allowed media root `{path}` is not a directory")]
    RootNotDirectory { path: PathBuf },
    #[error("failed to determine the current directory: {source}")]
    CurrentDirectory { source: io::Error },
    #[error("failed to inspect filesystem path `{path}`: {source}")]
    Inspect { path: PathBuf, source: io::Error },
    #[error("failed to canonicalize filesystem path `{path}`: {source}")]
    Canonicalize { path: PathBuf, source: io::Error },
    #[error("filesystem path `{path}` has no existing ancestor")]
    NoExistingAncestor { path: PathBuf },
    #[error("filesystem path `{path}` is outside the allowed media roots")]
    OutsideAllowedRoots { path: PathBuf },
}
