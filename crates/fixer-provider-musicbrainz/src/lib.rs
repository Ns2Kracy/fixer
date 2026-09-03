//! `MusicBrainz` album metadata provider.

#![forbid(unsafe_code)]

mod config;
mod error;
mod music;
mod provider;

pub use config::MusicBrainzConfig;
pub use error::MusicBrainzError;
pub use provider::MusicBrainzProvider;
