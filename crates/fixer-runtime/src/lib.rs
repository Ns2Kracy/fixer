#![forbid(unsafe_code)]

mod config;
mod runtime;

pub use config::{
    AniListConfig, ConfigHandle, ConfigLoadError, ConfigLoader, ConfigOverrides, ConfigWriteError,
    ConflictPolicy, EndpointConfig, FixerConfig, LoadedConfig, LoggingConfig, LoggingFormat,
    OpenLibraryConfig, OutputPreset, PlacementPolicy, ProviderEndpoints, ProvidersConfig,
    SecretString, ServerConfig, TmdbConfig, TrustedProxyConfig,
};
pub use runtime::{RuntimeConfigError, build_fixer};
