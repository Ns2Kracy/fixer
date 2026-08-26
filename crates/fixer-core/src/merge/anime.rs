//! Anime metadata merging with complementary alternate-title and artwork support.

use super::{FieldPath, MergeError, MergePolicy};
use crate::{
    AnimeSeries, ArtworkReference, LocalizedEntry, LocalizedValue, MediaKind, ProvenanceMap,
    ResolutionWarning, Resolved, SourceRef,
};
use std::collections::BTreeSet;

/// An anime document paired with provider source metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimeDocument {
    pub value: AnimeSeries,
    pub source: SourceRef,
}

impl AnimeDocument {
    pub const fn new(value: AnimeSeries, source: SourceRef) -> Self {
        Self { value, source }
    }
}

/// Deterministic anime merger preserving complementary values and hierarchy semantics.
#[derive(Debug, Clone)]
pub struct AnimeMerger {
    policy: MergePolicy,
}

impl AnimeMerger {
    /// Constructs a merger with explicit provider precedence.
    pub const fn new(policy: MergePolicy) -> Self {
        Self { policy }
    }

    /// Merges anime documents with field-level provenance.
    pub fn merge(
        &self,
        documents: impl IntoIterator<Item = AnimeDocument>,
    ) -> Result<Resolved<AnimeSeries>, MergeError> {
        let documents = documents.into_iter().collect::<Vec<_>>();
        if documents.is_empty() {
            return Err(MergeError::NoDocuments);
        }
        let ordered_base = self.ordered(&documents, "anime")?;
        let base = ordered_base
            .iter()
            .copied()
            .find(|document| !document.value.cours.is_empty())
            .unwrap_or(ordered_base[0]);
        let mut merged = base.value.clone();
        let mut provenance = ProvenanceMap::new();

        merged.titles = self.merge_localized(
            &documents,
            "anime.titles",
            |anime| &anime.titles,
            &mut provenance,
        )?;
        merged.summaries = self.merge_localized(
            &documents,
            "anime.summaries",
            |anime| &anime.summaries,
            &mut provenance,
        )?;
        merged.artwork = self.merge_artwork(&documents, &mut provenance)?;
        if !merged.cours.is_empty() {
            provenance.add("anime.cours", base.source.clone())?;
        }
        provenance.add("anime.relation", base.source.clone())?;

        let completeness = anime_completeness(&merged);
        let warnings = if completeness < 1.0 {
            vec![ResolutionWarning {
                code: "incomplete_metadata".to_owned(),
                message: format!("anime metadata is {:.0}% complete", completeness * 100.0),
            }]
        } else {
            Vec::new()
        };
        Ok(Resolved {
            value: merged,
            provenance,
            conflicts: Vec::new(),
            completeness,
            warnings,
        })
    }

    fn merge_localized(
        &self,
        documents: &[AnimeDocument],
        path: &str,
        select: impl Fn(&AnimeSeries) -> &LocalizedValue<String>,
        provenance: &mut ProvenanceMap,
    ) -> Result<LocalizedValue<String>, MergeError> {
        let ordered = self.ordered(documents, path)?;
        let mut result = LocalizedValue::new();
        let mut seen = BTreeSet::new();
        for document in ordered {
            for entry in select(&document.value).entries() {
                let identity = normalize(entry.value());
                if !seen.insert(identity) {
                    continue;
                }
                match entry {
                    LocalizedEntry::Tagged { language, value } => {
                        result.insert(language.as_str(), value.clone())?;
                    }
                    LocalizedEntry::Untagged { value } => result.insert_untagged(value.clone()),
                }
                provenance.add(path, document.source.clone())?;
            }
        }
        Ok(result)
    }

    fn merge_artwork(
        &self,
        documents: &[AnimeDocument],
        provenance: &mut ProvenanceMap,
    ) -> Result<Vec<ArtworkReference>, MergeError> {
        let ordered = self.ordered(documents, "anime.artwork")?;
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for document in ordered {
            for artwork in &document.value.artwork {
                if !seen.insert(artwork_key(artwork)) {
                    continue;
                }
                result.push(artwork.clone());
                provenance.add("anime.artwork", document.source.clone())?;
            }
        }
        Ok(result)
    }

    fn ordered<'a>(
        &self,
        documents: &'a [AnimeDocument],
        path: &str,
    ) -> Result<Vec<&'a AnimeDocument>, MergeError> {
        let field_path = FieldPath::new(path)?;
        let mut ordered = documents.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|document| {
            self.policy
                .rank(MediaKind::Anime, &field_path, &document.source.provider)
        });
        Ok(ordered)
    }
}

fn artwork_key(artwork: &ArtworkReference) -> String {
    artwork.external_id.as_ref().map_or_else(
        || {
            format!(
                "identity:{:?}:{}",
                artwork.kind,
                normalize(&artwork.location)
            )
        },
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

fn anime_completeness(anime: &AnimeSeries) -> f32 {
    let present = [
        !anime.titles.entries().is_empty(),
        !anime.cours.is_empty(),
        !anime.artwork.is_empty(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    present as f32 / 3.0
}
