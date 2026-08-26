//! Open Library book metadata provider.

#![forbid(unsafe_code)]

mod book;
mod config;
mod error;
mod provider;

pub use config::OpenLibraryConfig;
pub use error::OpenLibraryError;
pub use provider::OpenLibraryProvider;
