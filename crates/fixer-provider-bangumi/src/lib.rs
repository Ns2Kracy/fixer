//! Bangumi anime metadata provider.

#![forbid(unsafe_code)]

mod anime;
mod config;
mod error;
mod provider;

pub use config::BangumiConfig;
pub use error::BangumiError;
pub use provider::BangumiProvider;
