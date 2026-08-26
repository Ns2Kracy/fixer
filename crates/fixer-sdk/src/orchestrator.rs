//! Internal concurrent provider orchestration.

use crate::{Fixer, SdkError};
use fixer_core::{
    AnimeDocument, AnimeMerger, AnimeSeries, Candidate, ExternalId, FetchRequest, MatchQuery,
    Matcher, MediaKind, MergePolicy, MetadataDocument, MovieDocument, MovieMerger, OrderingScheme,
    ProvenanceMap, ResolutionWarning, Resolved, SearchRequest, SeriesDocument, SeriesMerger,
    SourceRef,
};
use futures_util::future::join_all;
use std::time::SystemTime;

pub(crate) struct SearchOutcome {
    pub candidates: Vec<Candidate>,
    pub warnings: Vec<ResolutionWarning>,
}

struct SourcedMetadata {
    document: MetadataDocument,
    source: SourceRef,
}

pub(crate) async fn search_movie(
    fixer: &Fixer,
    title: &str,
    year: Option<u16>,
) -> Result<SearchOutcome, SdkError> {
    let request =
        SearchRequest::movie(title, year)?.with_locales(fixer.preferred_languages.to_vec());
    let mut query = MatchQuery::movie(title)?;
    if let Some(year) = year {
        query = query.with_year(year);
    }
    search_candidates(fixer, MediaKind::Movie, request, query).await
}

pub(crate) async fn search_anime(
    fixer: &Fixer,
    title: &str,
    year: Option<u16>,
    external_ids: &[ExternalId],
) -> Result<SearchOutcome, SdkError> {
    let request =
        SearchRequest::anime(title, year)?.with_locales(fixer.preferred_languages.to_vec());
    let mut query = MatchQuery::anime(title)?;
    if let Some(year) = year {
        query = query.with_year(year);
    }
    for external_id in external_ids {
        query = query.with_external_id(external_id.clone());
    }
    search_candidates(fixer, MediaKind::Anime, request, query).await
}

pub(crate) async fn search_music(
    fixer: &Fixer,
    title: &str,
    year: Option<u16>,
) -> Result<SearchOutcome, SdkError> {
    let request =
        SearchRequest::music(title, year)?.with_locales(fixer.preferred_languages.to_vec());
    let mut query = MatchQuery::music(title)?;
    if let Some(year) = year {
        query = query.with_year(year);
    }
    search_candidates(fixer, MediaKind::Music, request, query).await
}

pub(crate) async fn search_television(
    fixer: &Fixer,
    title: &str,
    year: Option<u16>,
    external_ids: &[ExternalId],
) -> Result<SearchOutcome, SdkError> {
    let request =
        SearchRequest::television(title, year)?.with_locales(fixer.preferred_languages.to_vec());
    let mut query = MatchQuery::television(title)?;
    if let Some(year) = year {
        query = query.with_year(year);
    }
    for external_id in external_ids {
        query = query.with_external_id(external_id.clone());
    }
    search_candidates(fixer, MediaKind::Television, request, query).await
}

async fn search_candidates(
    fixer: &Fixer,
    media_kind: MediaKind,
    request: SearchRequest,
    query: MatchQuery,
) -> Result<SearchOutcome, SdkError> {
    let skipped_network = fixer.offline
        && fixer.providers.iter().any(|provider| {
            provider.descriptor().supports(media_kind) && provider.descriptor().requires_network()
        });
    let futures = fixer
        .providers
        .iter()
        .filter(|provider| provider.descriptor().supports(media_kind))
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
            Ok(found) => candidates.extend(
                found
                    .into_iter()
                    .filter(|candidate| candidate.media_kind() == media_kind),
            ),
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
    warnings: Vec<ResolutionWarning>,
) -> Result<Resolved<fixer_core::Movie>, SdkError> {
    let (documents, warnings) =
        fetch_metadata(fixer, MediaKind::Movie, candidates, warnings, &[]).await?;
    let documents = documents
        .into_iter()
        .map(|metadata| match metadata.document {
            MetadataDocument::Movie(movie) => Ok(MovieDocument::new(movie, metadata.source)),
            _ => Err(SdkError::UnexpectedDocument),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let policy = merge_policy(fixer);
    let mut resolved = MovieMerger::new(policy)
        .merge(documents)
        .map_err(|error| SdkError::Merge(error.to_string()))?;
    resolved.warnings.extend(warnings);
    Ok(resolved)
}

pub(crate) async fn fetch_anime(
    fixer: &Fixer,
    candidates: &[Candidate],
    warnings: Vec<ResolutionWarning>,
    identity_ids: &[ExternalId],
) -> Result<Resolved<AnimeSeries>, SdkError> {
    let (documents, warnings) =
        fetch_metadata(fixer, MediaKind::Anime, candidates, warnings, identity_ids).await?;
    let documents = documents
        .into_iter()
        .map(|metadata| match metadata.document {
            MetadataDocument::Anime(anime) => Ok(AnimeDocument::new(anime, metadata.source)),
            _ => Err(SdkError::UnexpectedDocument),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let policy = merge_policy(fixer);
    let mut resolved = AnimeMerger::new(policy)
        .merge(documents)
        .map_err(|error| SdkError::Merge(error.to_string()))?;
    resolved.warnings.extend(warnings);
    Ok(resolved)
}

pub(crate) async fn fetch_music(
    fixer: &Fixer,
    candidates: &[Candidate],
    warnings: Vec<ResolutionWarning>,
) -> Result<Resolved<fixer_core::MusicReleaseGroup>, SdkError> {
    let candidate = candidates.first().ok_or(SdkError::NoCandidates)?;
    let (mut documents, warnings) = fetch_metadata(
        fixer,
        MediaKind::Music,
        std::slice::from_ref(candidate),
        warnings,
        &[],
    )
    .await?;
    let metadata = documents.pop().ok_or(SdkError::NoCandidates)?;
    let MetadataDocument::Music(group) = metadata.document else {
        return Err(SdkError::UnexpectedDocument);
    };
    let mut provenance = ProvenanceMap::new();
    for field in [
        "music.release_group",
        "music.titles",
        "music.artist",
        "music.releases",
        "music.discs",
        "music.tracks",
    ] {
        provenance.add(field, metadata.source.clone())?;
    }
    Ok(Resolved {
        value: group,
        provenance,
        conflicts: Vec::new(),
        completeness: 1.0,
        warnings,
    })
}

pub(crate) async fn fetch_series(
    fixer: &Fixer,
    candidates: &[Candidate],
    warnings: Vec<ResolutionWarning>,
    ordering: Option<OrderingScheme>,
    identity_ids: &[ExternalId],
) -> Result<Resolved<fixer_core::Series>, SdkError> {
    let (documents, mut warnings) = fetch_metadata(
        fixer,
        MediaKind::Television,
        candidates,
        warnings,
        identity_ids,
    )
    .await?;
    let mut documents = documents
        .into_iter()
        .map(|metadata| match metadata.document {
            MetadataDocument::Television(series) => {
                Ok(SeriesDocument::new(series, metadata.source))
            }
            _ => Err(SdkError::UnexpectedDocument),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let selected_ordering = ordering.unwrap_or(documents[0].value.ordering);
    if ordering.is_some()
        && !documents
            .iter()
            .any(|document| document.value.ordering == selected_ordering)
    {
        return Err(SdkError::OrderingUnavailable {
            requested: selected_ordering,
        });
    }
    let before = documents.len();
    documents.retain(|document| document.value.ordering == selected_ordering);
    if documents.len() != before {
        warnings.push(ResolutionWarning {
            code: "ordering_incompatible_source".to_owned(),
            message: format!(
                "ignored metadata sources that do not use {selected_ordering:?} ordering"
            ),
        });
    }
    let policy = merge_policy(fixer);
    let mut resolved = SeriesMerger::new(policy)
        .merge(documents)
        .map_err(|error| SdkError::Merge(error.to_string()))?;
    resolved.warnings.extend(warnings);
    Ok(resolved)
}

async fn fetch_metadata(
    fixer: &Fixer,
    media_kind: MediaKind,
    candidates: &[Candidate],
    mut warnings: Vec<ResolutionWarning>,
    identity_ids: &[ExternalId],
) -> Result<(Vec<SourcedMetadata>, Vec<ResolutionWarning>), SdkError> {
    let candidates = if matches!(media_kind, MediaKind::Television | MediaKind::Anime) {
        candidate_group(candidates, identity_ids)
    } else {
        candidates.iter().collect()
    };
    let futures = candidates.into_iter().map(|candidate| async move {
        let provider = fixer
            .providers
            .iter()
            .find(|provider| provider.descriptor().id() == candidate.provider())
            .ok_or_else(|| SdkError::ProviderNotFound(candidate.provider().clone()))?;
        let request = FetchRequest::new(media_kind, candidate.external_id().clone())
            .with_locales(fixer.preferred_languages.to_vec());
        let document = provider.fetch(request, fixer.http.as_ref()).await?;
        if document.media_kind() != media_kind {
            return Err(SdkError::UnexpectedDocument);
        }
        Ok::<_, SdkError>(SourcedMetadata {
            document,
            source: SourceRef::new(
                provider.descriptor().id().clone(),
                Some(candidate.external_id().clone()),
                None,
                SystemTime::now(),
            ),
        })
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
    Ok((documents, warnings))
}

fn candidate_group<'a>(
    candidates: &'a [Candidate],
    identity_ids: &[ExternalId],
) -> Vec<&'a Candidate> {
    let Some(primary) = candidates.first() else {
        return Vec::new();
    };
    let mut selected = vec![primary];
    for candidate in candidates.iter().skip(1) {
        if selected
            .iter()
            .any(|item| item.provider() == candidate.provider())
        {
            continue;
        }
        if identity_ids.contains(candidate.external_id()) || same_work(primary, candidate) {
            selected.push(candidate);
        }
    }
    selected
}

fn same_work(left: &Candidate, right: &Candidate) -> bool {
    if left.external_id() == right.external_id() {
        return true;
    }
    let (left_title, left_year, left_sequence) = candidate_match_fields(left);
    let (right_title, right_year, right_sequence) = candidate_match_fields(right);
    normalize_title(left_title) == normalize_title(right_title)
        && (left_year.is_none() || right_year.is_none() || left_year == right_year)
        && (left_sequence.is_none() || right_sequence.is_none() || left_sequence == right_sequence)
}

fn candidate_match_fields(candidate: &Candidate) -> (&str, Option<u16>, Option<&str>) {
    match candidate {
        Candidate::Movie(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Television(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Anime(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Music(value) => (&value.title, value.year, value.sequence.as_deref()),
        Candidate::Book(value) => (&value.title, value.year, value.sequence.as_deref()),
    }
}

fn normalize_title(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn merge_policy(fixer: &Fixer) -> MergePolicy {
    MergePolicy::new(
        fixer
            .providers
            .iter()
            .map(|provider| provider.descriptor().id().clone()),
    )
}
