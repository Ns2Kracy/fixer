use fixer_core::{MediaKind, ScrapeSelection};

#[test]
fn tmdb_targets_accept_movies_and_television() {
    for media_kind in [MediaKind::Movie, MediaKind::Television] {
        let target = fixer_core::ProviderTarget::tmdb(media_kind, "329865").unwrap();

        assert_eq!(target.media_kind(), media_kind);
        assert_eq!(target.provider().as_str(), "tmdb");
        assert_eq!(target.external_id().namespace, "tmdb");
        assert_eq!(target.external_id().value, "329865");
        assert_eq!(ScrapeSelection::Exact(target).to_string_mode(), "exact");
    }
}

#[test]
fn tmdb_targets_reject_unsupported_media_and_invalid_ids() {
    assert!(fixer_core::ProviderTarget::tmdb(MediaKind::Anime, "329865").is_err());

    for id in ["", "0", "-1", "12.5", "tt329865"] {
        assert!(fixer_core::ProviderTarget::tmdb(MediaKind::Movie, id).is_err());
    }
}

trait SelectionMode {
    fn to_string_mode(&self) -> &'static str;
}

impl SelectionMode for ScrapeSelection {
    fn to_string_mode(&self) -> &'static str {
        match self {
            Self::Automatic => "automatic",
            Self::Exact(_) => "exact",
        }
    }
}

#[test]
fn scrape_selection_has_a_stable_tagged_json_shape() {
    let automatic = serde_json::to_value(ScrapeSelection::Automatic).unwrap();
    assert_eq!(automatic, serde_json::json!({ "mode": "automatic" }));

    let exact = ScrapeSelection::Exact(
        fixer_core::ProviderTarget::tmdb(MediaKind::Movie, "329865").unwrap(),
    );
    assert_eq!(
        serde_json::to_value(exact).unwrap(),
        serde_json::json!({
            "mode": "exact",
            "target": {
                "media_kind": "movie",
                "provider": "tmdb",
                "external_id": { "namespace": "tmdb", "value": "329865" }
            }
        })
    );
}
