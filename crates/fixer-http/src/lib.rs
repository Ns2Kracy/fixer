//! Rustls-backed default HTTP transport for Fixer.

#![forbid(unsafe_code)]

mod client;
mod config;

pub use client::ReqwestHttpClient;
pub use config::{HttpConfig, HttpConfigError};
