use axum::Json;
use serde::Serialize;

const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Serialize)]
pub(crate) struct ProvidersDto {
    schema_version: u8,
    providers: Vec<ProviderDto>,
}

#[derive(Debug, Serialize)]
pub(crate) struct ProviderDto {
    id: &'static str,
    name: &'static str,
    media_kinds: &'static [&'static str],
    network: bool,
    optional: bool,
}

pub(crate) async fn get() -> Json<ProvidersDto> {
    Json(ProvidersDto {
        schema_version: SCHEMA_VERSION,
        providers: vec![
            provider(
                "local",
                "Local metadata",
                &["movie", "television", "anime", "music", "book"],
                false,
                false,
            ),
            provider(
                "tmdb",
                "The Movie Database",
                &["movie", "television"],
                true,
                false,
            ),
            provider("bangumi", "Bangumi", &["anime"], true, false),
            provider("anilist", "AniList", &["anime"], true, true),
            provider("musicbrainz", "MusicBrainz", &["music"], true, false),
            provider("openlibrary", "Open Library", &["book"], true, false),
        ],
    })
}

const fn provider(
    id: &'static str,
    name: &'static str,
    media_kinds: &'static [&'static str],
    network: bool,
    optional: bool,
) -> ProviderDto {
    ProviderDto {
        id,
        name,
        media_kinds,
        network,
        optional,
    }
}
