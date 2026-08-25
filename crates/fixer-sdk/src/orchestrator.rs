//! Internal concurrent provider orchestration.

use crate::{Fixer, SdkError};
use fixer_core::{
    Candidate, FetchRequest, MatchQuery, Matcher, MediaKind, MergePolicy, MetadataDocument,
    MovieDocument, MovieMerger, ResolutionWarning, Resolved, SearchRequest, SourceRef,
};
use futures_util::future::join_all;
use std::time::SystemTime;

pub(crate) struct SearchOutcome {
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<ResolutionWarning>,
}

pub(crate) async fn search_movie(
    fixer: &Fixer,
    title: &str,
    year: Option<u16>,
) -> Result<SearchOutcome, SdkError> {
    let request =
        SearchRequest::movie(title, year)?.with_locales(fixer.preferred_languages.to_vec());
    let skipped_network = fixer.offline
        && fixer.providers.iter().any(|provider| {
            provider.descriptor().supports(MediaKind::Movie)
                && provider.descriptor().requires_network()
        });
    let futures = fixer
        .providers
        .iter()
        .filter(|provider| provider.descriptor().supports(MediaKind::Movie))
        .filter(|provider| !fixer.offline || !provider.descriptor().requires_network())
        .map(|provider| provider.search(request.clone(), fixer.http.as_ref()));
    let results = join_all(futures).await;
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    if skipped_network {
        warnings.push(ResolutionWarning {
            code: "offline_provider_skipped".to_owned(),
            message: "offline mode skipped one or more network providers".to_owned(),
        });
    }
    for result in results {
        match result {
            Ok(found) => candidates.extend(found),
            Err(error) => warnings.push(ResolutionWarning {
                code: "provider_search_failed".to_owned(),
                message: error.to_string(),
            }),
        }
    }
    if candidates.is_empty() {
        if warnings.is_empty() {
            return Err(SdkError::NoCandidates);
        }
        return Err(SdkError::AllProvidersFailed(
            warnings
                .into_iter()
                .map(|warning| warning.message)
                .collect(),
        ));
    }
    let mut query = MatchQuery::movie(title)?;
    if let Some(year) = year {
        query = query.with_year(year);
    }
    let selection = Matcher
        .select(&query, candidates)
        .map_err(|error| SdkError::Merge(error.to_string()))?;
    if selection.is_ambiguous() {
        warnings.push(ResolutionWarning {
            code: "ambiguous_candidates".to_owned(),
            message:
                "multiple candidates share the top score; provider order was used deterministically"
                    .to_owned(),
        });
    }
    Ok(SearchOutcome {
        candidates: selection
            .ranked()
            .iter()
            .map(|item| item.candidate.clone())
            .collect(),
        warnings,
    })
}

pub(crate) async fn fetch_movies(
    fixer: &Fixer,
    candidates: &[Candidate],
    mut warnings: Vec<ResolutionWarning>,
) -> Result<Resolved<fixer_core::Movie>, SdkError> {
    let futures = candidates.iter().map(|candidate| async move {
        let provider = fixer
            .providers
            .iter()
            .find(|provider| provider.descriptor().id() == candidate.provider())
            .ok_or_else(|| SdkError::ProviderNotFound(candidate.provider().clone()))?;
        let request = FetchRequest::new(MediaKind::Movie, candidate.external_id().clone())
            .with_locales(fixer.preferred_languages.to_vec());
        let document = provider.fetch(request, fixer.http.as_ref()).await?;
        let MetadataDocument::Movie(movie) = document else {
            return Err(SdkError::UnexpectedDocument);
        };
        Ok::<_, SdkError>(MovieDocument::new(
            movie,
            SourceRef::new(
                provider.descriptor().id().clone(),
                Some(candidate.external_id().clone()),
                None,
                SystemTime::now(),
            ),
        ))
    });
    let results = join_all(futures).await;
    let mut documents = Vec::new();
    for result in results {
        match result {
            Ok(document) => documents.push(document),
            Err(error) => warnings.push(ResolutionWarning {
                code: "provider_fetch_failed".to_owned(),
                message: error.to_string(),
            }),
        }
    }
    if documents.is_empty() {
        return Err(SdkError::AllProvidersFailed(
            warnings
                .into_iter()
                .map(|warning| warning.message)
                .collect(),
        ));
    }
    let policy = MergePolicy::new(
        fixer
            .providers
            .iter()
            .map(|provider| provider.descriptor().id().clone()),
    );
    let mut resolved = MovieMerger::new(policy)
        .merge(documents)
        .map_err(|error| SdkError::Merge(error.to_string()))?;
    resolved.warnings.extend(warnings);
    Ok(resolved)
}
