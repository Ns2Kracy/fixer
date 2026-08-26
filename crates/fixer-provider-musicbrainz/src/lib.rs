//! MusicBrainz album metadata provider.

#![forbid(unsafe_code)]

mod config;
mod error;

pub use config::MusicBrainzConfig;
pub use error::MusicBrainzError;
