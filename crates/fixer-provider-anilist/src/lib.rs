//! Optional `AniList` anime metadata provider.

#![forbid(unsafe_code)]

mod config;
mod error;
mod graphql;
mod provider;

pub use config::AniListConfig;
pub use error::AniListError;
pub use provider::AniListProvider;
