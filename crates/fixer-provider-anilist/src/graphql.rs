use crate::{AniListConfig, AniListError};
use fixer_core::{
    AnimeCandidate, AnimeSeries, AnimeSeriesRelation, ArtworkKind, ArtworkReference, Candidate,
    ExternalId, FetchRequest, Header, HttpClient, HttpMethod, HttpRequest, LocalizedValue,
    MediaKind, ProviderId, SearchRequest, WorkId,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const SEARCH_QUERY: &str = r#"
query SearchAnime($search: String!) {
  Page(page: 1, perPage: 25) {
    media(search: $search, type: ANIME, sort: SEARCH_MATCH) {
      id
      title { romaji english native }
      synonyms
      seasonYear
      description(asHtml: false)
      coverImage { extraLarge large }
      bannerImage
    }
  }
}
"#;

const FETCH_QUERY: &str = r#"
query FetchAnime($id: Int!) {
  Media(id: $id, type: ANIME) {
    id
    title { romaji english native }
    synonyms
    seasonYear
    format
    description(asHtml: false)
    coverImage { extraLarge large }
    bannerImage
  }
}
"#;

#[derive(Debug, Serialize)]
struct GraphQlRequest<V> {
    query: &'static str,
    variables: V,
}

#[derive(Debug, Serialize)]
struct SearchVariables<'a> {
    search: &'a str,
}

#[derive(Debug, Serialize)]
struct FetchVariables {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct GraphQlEnvelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<GraphQlError>,
}

#[derive(Debug, Deserialize)]
struct GraphQlError {
    message: String,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    #[serde(rename = "Page")]
    page: MediaPage,
}

#[derive(Debug, Deserialize)]
struct MediaPage {
    #[serde(default)]
    media: Vec<Media>,
}

#[derive(Debug, Deserialize)]
struct FetchData {
    #[serde(rename = "Media")]
    media: Option<Media>,
}

#[derive(Debug, Deserialize)]
struct Media {
    id: u64,
    title: MediaTitle,
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(rename = "seasonYear")]
    season_year: Option<u16>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "coverImage", default)]
    cover_image: Option<CoverImage>,
    #[serde(rename = "bannerImage", default)]
    banner_image: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MediaTitle {
    romaji: Option<String>,
    english: Option<String>,
    native: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CoverImage {
    #[serde(rename = "extraLarge")]
    extra_large: Option<String>,
    large: Option<String>,
}

pub async fn search(
    config: &AniListConfig,
    request: SearchRequest,
    http: &dyn HttpClient,
) -> Result<Vec<Candidate>, AniListError> {
    let SearchRequest::Anime { title, year, .. } = request else {
        return Err(AniListError::InvalidData(
            "AniList search requires an anime request".to_owned(),
        ));
    };
    let data: SearchData = execute(
        config,
        GraphQlRequest {
            query: SEARCH_QUERY,
            variables: SearchVariables { search: &title },
        },
        http,
    )
    .await?;

    data.page
        .media
        .into_iter()
        .filter(|media| year.is_none() || media.season_year == year)
        .map(|media| {
            AnimeCandidate::new(
                ProviderId::new("anilist").map_err(data_error)?,
                ExternalId::new("anilist", media.id.to_string()).map_err(data_error)?,
                candidate_title(&media).ok_or_else(|| {
                    AniListError::InvalidData(format!(
                        "AniList media {} has no usable title",
                        media.id
                    ))
                })?,
                media.season_year,
            )
            .map(Candidate::Anime)
            .map_err(data_error)
        })
        .collect()
}

pub async fn fetch(
    config: &AniListConfig,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<AnimeSeries, AniListError> {
    if request.media_kind != MediaKind::Anime || request.external_id.namespace != "anilist" {
        return Err(AniListError::InvalidData(
            "AniList fetch requires a numeric anilist anime ID".to_owned(),
        ));
    }
    let id =
        request.external_id.value.parse::<u64>().map_err(|_| {
            AniListError::InvalidData("AniList anime ID must be numeric".to_owned())
        })?;
    let data: FetchData = execute(
        config,
        GraphQlRequest {
            query: FETCH_QUERY,
            variables: FetchVariables { id },
        },
        http,
    )
    .await?;
    let media = data.media.ok_or(AniListError::NotFound)?;
    if media.id != id {
        return Err(AniListError::InvalidData(
            "AniList response did not match the requested anime".to_owned(),
        ));
    }
    map_media(media)
}

async fn execute<V: Serialize, T: DeserializeOwned>(
    config: &AniListConfig,
    request: GraphQlRequest<V>,
    http: &dyn HttpClient,
) -> Result<T, AniListError> {
    let body = serde_json::to_vec(&request)
        .map_err(|error| AniListError::InvalidData(error.to_string()))?;
    let mut request = HttpRequest::new(HttpMethod::Post, config.endpoint().to_string())
        .with_header(Header::new("accept", "application/json").map_err(data_error)?)
        .with_header(Header::new("content-type", "application/json").map_err(data_error)?)
        .with_body(body);
    if let Some(token) = config.access_token() {
        request = request.with_header(
            Header::new("authorization", format!("Bearer {token}")).map_err(data_error)?,
        );
    }
    let response = http
        .execute(request)
        .await
        .map_err(AniListError::from_http)?;
    let envelope: GraphQlEnvelope<T> = serde_json::from_slice(&response.body)
        .map_err(|error| AniListError::MalformedResponse(error.to_string()))?;
    if !envelope.errors.is_empty() {
        return Err(AniListError::GraphQl(
            envelope
                .errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; "),
        ));
    }
    envelope.data.ok_or_else(|| {
        AniListError::MalformedResponse(
            "GraphQL response contained neither data nor errors".to_owned(),
        )
    })
}

fn map_media(media: Media) -> Result<AnimeSeries, AniListError> {
    let mut titles = LocalizedValue::new();
    insert_title(&mut titles, "ja-Latn", media.title.romaji)?;
    insert_title(&mut titles, "en", media.title.english)?;
    if let Some(native) = media.title.native {
        let locale = infer_native_locale(&native);
        insert_title(&mut titles, locale, Some(native))?;
    }
    for synonym in media.synonyms {
        insert_untagged_title(&mut titles, synonym);
    }
    if titles.entries().is_empty() {
        return Err(AniListError::InvalidData(format!(
            "AniList media {} has no usable title",
            media.id
        )));
    }
    let mut anime = AnimeSeries::new(
        WorkId::new(format!("anilist-{}", media.id)).map_err(data_error)?,
        titles,
        AnimeSeriesRelation::Original,
        Vec::new(),
    );
    if let Some(description) = media.description.filter(|value| !value.trim().is_empty()) {
        anime
            .summaries
            .insert("en", description)
            .map_err(data_error)?;
    }
    if let Some(cover) = media
        .cover_image
        .and_then(|image| image.extra_large.or(image.large))
    {
        anime.artwork.push(
            ArtworkReference::new(ArtworkKind::Cover, cover)
                .map_err(data_error)?
                .with_external_id(
                    ExternalId::new("anilist-artwork", format!("{}-cover", media.id))
                        .map_err(data_error)?,
                ),
        );
    }
    if let Some(banner) = media.banner_image.filter(|value| !value.trim().is_empty()) {
        anime.artwork.push(
            ArtworkReference::new(ArtworkKind::Banner, banner)
                .map_err(data_error)?
                .with_external_id(
                    ExternalId::new("anilist-artwork", format!("{}-banner", media.id))
                        .map_err(data_error)?,
                ),
        );
    }
    Ok(anime)
}

fn candidate_title(media: &Media) -> Option<String> {
    media
        .title
        .english
        .as_ref()
        .or(media.title.romaji.as_ref())
        .or(media.title.native.as_ref())
        .filter(|value| !value.trim().is_empty())
        .cloned()
}

fn insert_title(
    titles: &mut LocalizedValue<String>,
    locale: &str,
    value: Option<String>,
) -> Result<(), AniListError> {
    let Some(value) = value else {
        return Ok(());
    };
    let value = value.trim().to_owned();
    if value.is_empty() || contains_title(titles, &value) {
        return Ok(());
    }
    titles.insert(locale, value).map_err(data_error)
}

fn insert_untagged_title(titles: &mut LocalizedValue<String>, value: String) {
    let value = value.trim().to_owned();
    if !value.is_empty() && !contains_title(titles, &value) {
        titles.insert_untagged(value);
    }
}

fn contains_title(titles: &LocalizedValue<String>, value: &str) -> bool {
    titles.entries().iter().any(|entry| entry.value() == value)
}

fn infer_native_locale(value: &str) -> &'static str {
    if value.chars().any(|character| {
        ('\u{3040}'..='\u{30ff}').contains(&character)
            || ('\u{31f0}'..='\u{31ff}').contains(&character)
    }) {
        "ja"
    } else {
        "und"
    }
}

fn data_error(error: impl std::fmt::Display) -> AniListError {
    AniListError::InvalidData(error.to_string())
}
