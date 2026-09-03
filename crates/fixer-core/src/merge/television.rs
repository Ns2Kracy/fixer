//! Television hierarchy merge with field-level provenance.

use super::{FieldPath, MergeError, MergePolicy};
use crate::{
    ArtworkReference, Credit, Episode, LocalizedEntry, LocalizedValue, MediaKind, MergeConflict,
    MetadataDocument, ProvenanceMap, ProviderId, ResolutionWarning, Resolved, Season, Series,
    SourceRef,
};
use std::collections::{BTreeMap, BTreeSet};

/// A television series document paired with provider source metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesDocument {
    pub value: Series,
    pub source: SourceRef,
}

impl SeriesDocument {
    /// Constructs a sourced television series document.
    pub const fn new(value: Series, source: SourceRef) -> Self {
        Self { value, source }
    }
}

/// Deterministic television hierarchy merger.
#[derive(Debug, Clone)]
pub struct SeriesMerger {
    policy: MergePolicy,
}

impl SeriesMerger {
    /// Constructs a merger with explicit provider precedence.
    pub const fn new(policy: MergePolicy) -> Self {
        Self { policy }
    }

    /// Merges series, seasons, and episodes while retaining field provenance.
    pub fn merge(
        &self,
        documents: impl IntoIterator<Item = SeriesDocument>,
    ) -> Result<Resolved<Series>, MergeError> {
        let mut documents = documents.into_iter().collect::<Vec<_>>();
        if documents.is_empty() {
            return Err(MergeError::NoDocuments);
        }
        sort_documents(&mut documents, "series", &self.policy)?;
        let ordering = documents[0].value.ordering;
        for document in &documents {
            if document.value.ordering != ordering {
                return Err(MergeError::OrderingMismatch {
                    expected: ordering,
                    found: document.value.ordering,
                });
            }
            for episode in document
                .value
                .seasons
                .iter()
                .flat_map(|season| &season.episodes)
            {
                if episode.sequence.scheme != ordering {
                    return Err(MergeError::OrderingMismatch {
                        expected: ordering,
                        found: episode.sequence.scheme,
                    });
                }
            }
        }
        let mut merged = documents[0].value.clone();
        let mut provenance = ProvenanceMap::new();
        let mut conflicts = Vec::new();

        merged.titles = merge_series_localized(
            "series.titles",
            &documents,
            |series| &series.titles,
            &self.policy,
            &mut provenance,
            &mut conflicts,
        )?;
        merged.summaries = merge_series_localized(
            "series.summaries",
            &documents,
            |series| &series.summaries,
            &self.policy,
            &mut provenance,
            &mut conflicts,
        )?;
        merged.artwork = merge_series_artwork(&documents, &self.policy, &mut provenance)?;
        merged.seasons = merge_seasons(&documents, &self.policy, &mut provenance, &mut conflicts)?;

        provenance.add("series.ordering", documents[0].source.clone())?;

        let completeness = series_completeness(&merged);
        let warnings = if completeness < 1.0 {
            vec![ResolutionWarning {
                code: "incomplete_metadata".to_owned(),
                message: format!(
                    "television metadata is {:.0}% complete",
                    completeness * 100.0
                ),
            }]
        } else {
            Vec::new()
        };
        Ok(Resolved {
            value: merged,
            provenance,
            conflicts,
            completeness,
            warnings,
        })
    }

    /// Rejects bare documents because merge provenance requires source metadata.
    pub fn merge_documents(
        &self,
        documents: impl IntoIterator<Item = MetadataDocument>,
    ) -> Result<Resolved<Series>, MergeError> {
        let mut saw_series = false;
        for document in documents {
            match document {
                MetadataDocument::Television(_) => saw_series = true,
                other => return Err(MergeError::UnsupportedDocument(other.media_kind())),
            }
        }
        if saw_series {
            Err(MergeError::MissingSource)
        } else {
            Err(MergeError::NoDocuments)
        }
    }
}

fn sort_documents(
    documents: &mut [SeriesDocument],
    path: &str,
    policy: &MergePolicy,
) -> Result<(), MergeError> {
    let path = FieldPath::new(path)?;
    documents.sort_by_key(|document| {
        policy.rank(MediaKind::Television, &path, &document.source.provider)
    });
    Ok(())
}

fn ordered_series_documents<'a>(
    documents: &'a [SeriesDocument],
    path: &str,
    policy: &MergePolicy,
) -> Result<Vec<&'a SeriesDocument>, MergeError> {
    let path = FieldPath::new(path)?;
    let mut ordered = documents.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|document| {
        policy.rank(MediaKind::Television, &path, &document.source.provider)
    });
    Ok(ordered)
}

fn merge_series_localized(
    path: &str,
    documents: &[SeriesDocument],
    select: impl Fn(&Series) -> &LocalizedValue<String>,
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<LocalizedValue<String>, MergeError> {
    let ordered = ordered_series_documents(documents, path, policy)?;
    merge_localized(
        path,
        ordered
            .into_iter()
            .map(|document| (select(&document.value), &document.source)),
        provenance,
        conflicts,
    )
}

fn merge_localized<'a>(
    path: &str,
    values: impl IntoIterator<Item = (&'a LocalizedValue<String>, &'a SourceRef)>,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<LocalizedValue<String>, MergeError> {
    let mut result = LocalizedValue::new();
    let mut identities = BTreeMap::<String, (String, ProviderId)>::new();
    for (values, source) in values {
        for entry in values.entries() {
            let language = entry
                .language()
                .map_or_else(|| "untagged".to_owned(), |tag| tag.normalized().to_owned());
            if let Some((existing, provider)) = identities.get(&language) {
                if normalize(existing) != normalize(entry.value()) {
                    conflicts.push(MergeConflict {
                        field_path: format!("{path}.{language}"),
                        providers: vec![provider.clone(), source.provider.clone()],
                        message: "providers supplied different localized values".to_owned(),
                    });
                }
                continue;
            }
            identities.insert(language, (entry.value().clone(), source.provider.clone()));
            match entry {
                LocalizedEntry::Tagged { language, value } => {
                    result.insert(language.as_str(), value.clone())?;
                }
                LocalizedEntry::Untagged { value } => result.insert_untagged(value.clone()),
            }
            provenance.add(path, source.clone())?;
        }
    }
    Ok(result)
}

fn merge_series_artwork(
    documents: &[SeriesDocument],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<ArtworkReference>, MergeError> {
    let ordered = ordered_series_documents(documents, "series.artwork", policy)?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for document in ordered {
        for artwork in &document.value.artwork {
            if seen.insert(artwork_key(artwork)) {
                result.push(artwork.clone());
                provenance.add("series.artwork", document.source.clone())?;
            }
        }
    }
    Ok(result)
}

fn merge_seasons(
    documents: &[SeriesDocument],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<Vec<Season>, MergeError> {
    let mut numbers = BTreeSet::new();
    for document in documents {
        numbers.extend(document.value.seasons.iter().map(|season| season.number));
    }
    let mut result = Vec::new();
    for number in numbers {
        let season_path = format!("series.seasons.{number}");
        let ordered = ordered_series_documents(documents, &season_path, policy)?;
        let season_documents = ordered
            .into_iter()
            .filter_map(|document| {
                document
                    .value
                    .seasons
                    .iter()
                    .find(|season| season.number == number)
                    .map(|season| (document, season))
            })
            .collect::<Vec<_>>();
        let Some((base_document, base_season)) = season_documents.first() else {
            continue;
        };
        let mut season = (*base_season).clone();
        provenance.add(&season_path, base_document.source.clone())?;
        season.episodes = merge_episodes(number, &season_documents, policy, provenance, conflicts)?;
        season.artwork = merge_season_artwork(number, &season_documents, policy, provenance)?;
        result.push(season);
    }
    Ok(result)
}

fn merge_episodes(
    season_number: u32,
    season_documents: &[(&SeriesDocument, &Season)],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<Vec<Episode>, MergeError> {
    let mut sequences = BTreeSet::new();
    for (_, season) in season_documents {
        sequences.extend(season.episodes.iter().map(|episode| episode.sequence));
    }
    let mut result = Vec::new();
    for sequence in sequences {
        let episode_number = sequence.episode;
        let base_path = format!("series.seasons.{season_number}.episodes.{episode_number}");
        let field_path = FieldPath::new(&base_path)?;
        let mut episode_documents = season_documents
            .iter()
            .filter_map(|(document, season)| {
                season
                    .episodes
                    .iter()
                    .find(|episode| episode.sequence == sequence)
                    .map(|episode| (*document, episode))
            })
            .collect::<Vec<_>>();
        episode_documents.sort_by_key(|(document, _)| {
            policy.rank(
                MediaKind::Television,
                &field_path,
                &document.source.provider,
            )
        });
        let Some((base_document, base_episode)) = episode_documents.first() else {
            continue;
        };
        let mut episode = (*base_episode).clone();
        provenance.add(&base_path, base_document.source.clone())?;

        let titles_path = format!("{base_path}.titles");
        episode.titles = merge_episode_localized(
            &titles_path,
            &episode_documents,
            |episode| &episode.titles,
            policy,
            provenance,
            conflicts,
        )?;
        let summaries_path = format!("{base_path}.summaries");
        episode.summaries = merge_episode_localized(
            &summaries_path,
            &episode_documents,
            |episode| &episode.summaries,
            policy,
            provenance,
            conflicts,
        )?;
        episode.runtime =
            ordered_episode_documents(&episode_documents, &format!("{base_path}.runtime"), policy)?
                .into_iter()
                .find_map(|(document, episode)| {
                    episode.runtime.inspect(|_runtime| {
                        let _ =
                            provenance.add(format!("{base_path}.runtime"), document.source.clone());
                    })
                });
        episode.credits =
            merge_episode_credits(&base_path, &episode_documents, policy, provenance)?;
        episode.artwork =
            merge_episode_artwork(&base_path, &episode_documents, policy, provenance)?;
        result.push(episode);
    }
    Ok(result)
}

fn ordered_episode_documents<'a>(
    documents: &[(&'a SeriesDocument, &'a Episode)],
    path: &str,
    policy: &MergePolicy,
) -> Result<Vec<(&'a SeriesDocument, &'a Episode)>, MergeError> {
    let path = FieldPath::new(path)?;
    let mut ordered = documents.to_vec();
    ordered.sort_by_key(|(document, _)| {
        policy.rank(MediaKind::Television, &path, &document.source.provider)
    });
    Ok(ordered)
}

fn merge_episode_localized(
    path: &str,
    documents: &[(&SeriesDocument, &Episode)],
    select: impl Fn(&Episode) -> &LocalizedValue<String>,
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
    conflicts: &mut Vec<MergeConflict>,
) -> Result<LocalizedValue<String>, MergeError> {
    let ordered = ordered_episode_documents(documents, path, policy)?;
    merge_localized(
        path,
        ordered
            .into_iter()
            .map(|(document, episode)| (select(episode), &document.source)),
        provenance,
        conflicts,
    )
}

fn merge_episode_credits(
    base_path: &str,
    documents: &[(&SeriesDocument, &Episode)],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<Credit>, MergeError> {
    let path = format!("{base_path}.credits");
    let ordered = ordered_episode_documents(documents, &path, policy)?;
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut result = Vec::new();
    for (document, episode) in ordered {
        for credit in &episode.credits {
            let id = credit.person.id.as_str().to_owned();
            let identity = format!("{}:{:?}", normalize(&credit.person.name), credit.role);
            if ids.contains(&id) || identities.contains(&identity) {
                continue;
            }
            ids.insert(id);
            identities.insert(identity);
            result.push(credit.clone());
            provenance.add(&path, document.source.clone())?;
        }
    }
    Ok(result)
}

fn merge_season_artwork(
    season_number: u32,
    documents: &[(&SeriesDocument, &Season)],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<ArtworkReference>, MergeError> {
    let path = format!("series.seasons.{season_number}.artwork");
    let field_path = FieldPath::new(&path)?;
    let mut ordered = documents.to_vec();
    ordered.sort_by_key(|(document, _)| {
        policy.rank(
            MediaKind::Television,
            &field_path,
            &document.source.provider,
        )
    });
    merge_artwork_values(
        &path,
        ordered
            .into_iter()
            .map(|(document, season)| (season.artwork.as_slice(), &document.source)),
        provenance,
    )
}

fn merge_episode_artwork(
    base_path: &str,
    documents: &[(&SeriesDocument, &Episode)],
    policy: &MergePolicy,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<ArtworkReference>, MergeError> {
    let path = format!("{base_path}.artwork");
    let ordered = ordered_episode_documents(documents, &path, policy)?;
    merge_artwork_values(
        &path,
        ordered
            .into_iter()
            .map(|(document, episode)| (episode.artwork.as_slice(), &document.source)),
        provenance,
    )
}

fn merge_artwork_values<'a>(
    path: &str,
    values: impl IntoIterator<Item = (&'a [ArtworkReference], &'a SourceRef)>,
    provenance: &mut ProvenanceMap,
) -> Result<Vec<ArtworkReference>, MergeError> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (artwork, source) in values {
        for value in artwork {
            if seen.insert(artwork_key(value)) {
                result.push(value.clone());
                provenance.add(path, source.clone())?;
            }
        }
    }
    Ok(result)
}

fn artwork_key(value: &ArtworkReference) -> String {
    value.external_id.as_ref().map_or_else(
        || format!("identity:{:?}:{}", value.kind, normalize(&value.location)),
        |id| format!("id:{}:{}", id.namespace, id.value),
    )
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

#[allow(
    clippy::cast_precision_loss,
    reason = "the count is bounded by this fixed five-element completeness checklist"
)]
fn series_completeness(series: &Series) -> f32 {
    let present = [
        !series.titles.entries().is_empty(),
        !series.summaries.entries().is_empty(),
        !series.seasons.is_empty(),
        series
            .seasons
            .iter()
            .any(|season| !season.episodes.is_empty()),
        !series.artwork.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    present as f32 / 5.0
}
