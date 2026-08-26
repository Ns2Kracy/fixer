use crate::{MusicBrainzConfig, MusicBrainzError, music};
use fixer_core::{
    BoxFuture, Candidate, FetchRequest, HttpClient, MediaKind, MetadataDocument, MusicReleaseGroup,
    Provider, ProviderDescriptor, ProviderError, ProviderId, SearchRequest,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// MusicBrainz release-group provider with a shared request pacing gate.
#[derive(Debug, Clone)]
pub struct MusicBrainzProvider {
    descriptor: ProviderDescriptor,
    config: MusicBrainzConfig,
    gate: Arc<RequestGate>,
}

impl MusicBrainzProvider {
    /// Constructs a music-only provider with shared pacing across clones.
    pub fn new(config: MusicBrainzConfig) -> Result<Self, MusicBrainzError> {
        Ok(Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::new("musicbrainz")
                    .map_err(|error| MusicBrainzError::InvalidConfig(error.to_string()))?,
                "MusicBrainz",
                [MediaKind::Music],
            )
            .map_err(|error| MusicBrainzError::InvalidConfig(error.to_string()))?,
            gate: Arc::new(RequestGate::new(config.minimum_request_interval())),
            config,
        })
    }

    /// Searches MusicBrainz release groups.
    pub async fn search_music(
        &self,
        request: SearchRequest,
        http: &dyn HttpClient,
    ) -> Result<Vec<Candidate>, MusicBrainzError> {
        music::search(&self.config, &self.gate, request, http).await
    }

    /// Fetches a release group and one representative release hierarchy.
    pub async fn fetch_music(
        &self,
        request: FetchRequest,
        http: &dyn HttpClient,
    ) -> Result<MusicReleaseGroup, MusicBrainzError> {
        music::fetch(&self.config, &self.gate, request, http).await
    }
}

impl Provider for MusicBrainzProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        request: SearchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>> {
        Box::pin(async move {
            if request.media_kind() != MediaKind::Music {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.search_music(request, http)
                .await
                .map_err(ProviderError::from)
        })
    }

    fn fetch<'a>(
        &'a self,
        request: FetchRequest,
        http: &'a dyn HttpClient,
    ) -> BoxFuture<'a, Result<MetadataDocument, ProviderError>> {
        Box::pin(async move {
            if request.media_kind() != MediaKind::Music {
                return Err(ProviderError::UnsupportedMedia {
                    provider: self.descriptor.id().clone(),
                    media_kind: request.media_kind(),
                });
            }
            self.fetch_music(request, http)
                .await
                .map(MetadataDocument::Music)
                .map_err(ProviderError::from)
        })
    }
}

#[derive(Debug)]
pub(crate) struct RequestGate {
    interval: Duration,
    next_start: Mutex<Option<Instant>>,
}

impl RequestGate {
    const fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_start: Mutex::new(None),
        }
    }

    pub async fn wait(&self) -> Result<(), MusicBrainzError> {
        let now = Instant::now();
        let start = {
            let mut next = self.next_start.lock().map_err(|_| {
                MusicBrainzError::Transport("request pacing lock was poisoned".to_owned())
            })?;
            let start = next.map_or(now, |scheduled| scheduled.max(now));
            *next = Some(start + self.interval);
            start
        };
        if start > now {
            tokio::time::sleep(start.duration_since(now)).await;
        }
        Ok(())
    }
}
