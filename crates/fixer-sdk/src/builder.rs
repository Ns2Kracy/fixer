//! Validated SDK construction.

use crate::{Fixer, SdkError};
use fixer_core::{HttpClient, LanguageTag, Provider, ProviderId};
use std::{collections::BTreeSet, sync::Arc};

/// Builder for a [`Fixer`] instance.
///
/// # Examples
///
/// ```
/// use fixer_core::ProviderId;
/// use fixer_sdk::{FixtureDocument, FixtureProvider, Fixer};
///
/// let provider = FixtureProvider::new(
///     ProviderId::new("fixture")?,
///     Vec::<FixtureDocument>::new(),
/// )?;
/// let fixer = Fixer::builder()
///     .provider(provider)
///     .preferred_languages(["zh-CN", "en"])?
///     .build()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Default)]
pub struct FixerBuilder {
    providers: Vec<Arc<dyn Provider>>,
    preferred_languages: Vec<LanguageTag>,
    http: Option<Arc<dyn HttpClient>>,
}

impl FixerBuilder {
    /// Registers one compile-time provider implementation.
    pub fn provider(mut self, provider: impl Provider + 'static) -> Self {
        self.providers.push(Arc::new(provider));
        self
    }

    /// Replaces the ordered preferred language tags.
    pub fn preferred_languages<I, S>(mut self, tags: I) -> Result<Self, SdkError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.preferred_languages = tags
            .into_iter()
            .map(|tag| tag.as_ref().parse())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self)
    }

    /// Overrides the runtime-neutral HTTP client.
    pub fn http_client(mut self, http: impl HttpClient + 'static) -> Self {
        self.http = Some(Arc::new(http));
        self
    }

    /// Validates providers and constructs the SDK.
    pub fn build(self) -> Result<Fixer, SdkError> {
        if self.providers.is_empty() {
            return Err(SdkError::NoProviders);
        }
        let mut ids = BTreeSet::<ProviderId>::new();
        for provider in &self.providers {
            let id = provider.descriptor().id().clone();
            if !ids.insert(id.clone()) {
                return Err(SdkError::DuplicateProvider(id));
            }
        }
        Ok(Fixer::new(
            self.providers,
            self.preferred_languages,
            self.http,
        ))
    }
}
