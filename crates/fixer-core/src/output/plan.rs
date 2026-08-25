//! Serializable, previewable output plans.

use crate::CoreError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Content made available before plan execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedContent {
    bytes: Vec<u8>,
}
impl PlannedContent {
    /// Constructs planned bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
    }
    /// Returns the planned bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// One inspectable filesystem operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum OutputOperation {
    CreateDirectory {
        target: PathBuf,
    },
    WriteBytes {
        target: PathBuf,
        content: PlannedContent,
    },
    Copy {
        source: PathBuf,
        target: PathBuf,
    },
    Symlink {
        source: PathBuf,
        target: PathBuf,
    },
    Hardlink {
        source: PathBuf,
        target: PathBuf,
    },
    Reflink {
        source: PathBuf,
        target: PathBuf,
    },
}

impl OutputOperation {
    pub fn create_directory(target: impl Into<PathBuf>) -> Result<Self, CoreError> {
        Ok(Self::CreateDirectory {
            target: safe_target(target)?,
        })
    }
    pub fn write_bytes(
        target: impl Into<PathBuf>,
        content: PlannedContent,
    ) -> Result<Self, CoreError> {
        Ok(Self::WriteBytes {
            target: safe_target(target)?,
            content,
        })
    }
    pub fn copy(source: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Result<Self, CoreError> {
        Ok(Self::Copy {
            source: non_empty_path(source, "output.source")?,
            target: safe_target(target)?,
        })
    }
    pub fn symlink(
        source: impl Into<PathBuf>,
        target: impl Into<PathBuf>,
    ) -> Result<Self, CoreError> {
        Ok(Self::Symlink {
            source: non_empty_path(source, "output.source")?,
            target: safe_target(target)?,
        })
    }
    pub fn hardlink(
        source: impl Into<PathBuf>,
        target: impl Into<PathBuf>,
    ) -> Result<Self, CoreError> {
        Ok(Self::Hardlink {
            source: non_empty_path(source, "output.source")?,
            target: safe_target(target)?,
        })
    }
    pub fn reflink(
        source: impl Into<PathBuf>,
        target: impl Into<PathBuf>,
    ) -> Result<Self, CoreError> {
        Ok(Self::Reflink {
            source: non_empty_path(source, "output.source")?,
            target: safe_target(target)?,
        })
    }
    /// Returns the operation's source path when applicable.
    pub fn source(&self) -> Option<&Path> {
        match self {
            Self::Copy { source, .. }
            | Self::Symlink { source, .. }
            | Self::Hardlink { source, .. }
            | Self::Reflink { source, .. } => Some(source),
            Self::CreateDirectory { .. } | Self::WriteBytes { .. } => None,
        }
    }
    /// Returns the operation's target path.
    pub fn target(&self) -> Option<&Path> {
        Some(match self {
            Self::CreateDirectory { target }
            | Self::WriteBytes { target, .. }
            | Self::Copy { target, .. }
            | Self::Symlink { target, .. }
            | Self::Hardlink { target, .. }
            | Self::Reflink { target, .. } => target,
        })
    }
}

fn safe_target(value: impl Into<PathBuf>) -> Result<PathBuf, CoreError> {
    let value = non_empty_path(value, "output.target")?;
    if value.is_absolute()
        || value.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(CoreError::InvalidDomainValue {
            field: "output.target",
            value: value.display().to_string(),
        });
    }
    Ok(value)
}
fn non_empty_path(value: impl Into<PathBuf>, field: &'static str) -> Result<PathBuf, CoreError> {
    let value = value.into();
    if value.as_os_str().is_empty() {
        Err(CoreError::InvalidDomainValue {
            field,
            value: String::new(),
        })
    } else {
        Ok(value)
    }
}

/// An ordered set of output operations relative to one root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputPlan {
    pub output_root: PathBuf,
    operations: Vec<OutputOperation>,
}
impl OutputPlan {
    /// Constructs an empty output plan.
    pub fn new(output_root: impl Into<PathBuf>) -> Self {
        Self {
            output_root: output_root.into(),
            operations: Vec::new(),
        }
    }
    /// Appends an operation in execution order.
    pub fn push(&mut self, operation: OutputOperation) {
        self.operations.push(operation);
    }
    /// Returns planned operations in execution order.
    pub fn operations(&self) -> &[OutputOperation] {
        &self.operations
    }
}
