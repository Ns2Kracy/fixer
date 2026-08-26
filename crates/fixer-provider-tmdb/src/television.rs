use crate::{TmdbConfig, TmdbError};
use fixer_core::{
    ArtworkKind, ArtworkReference, Candidate, Credit, CreditRole, Duration, Episode,
    EpisodeSequence, ExternalId, FetchRequest, Header, HttpClient, HttpMethod, HttpRequest,
    LocalizedValue, MediaKind, OrderingScheme, Person, PersonId, SearchRequest, Season, Series,
    TelevisionCandidate, WorkId,
};
use serde::{Deserialize, de::DeserializeOwned};
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchSeries>,
}

#[derive(Deserialize)]
struct SearchSeries {
    id: u64,
    name: String,
    first_air_date: Option<String>,
}

#[derive(Deserialize)]
struct SeriesDetails {
    id: u64,
    name: String,
    original_name: String,
    original_language: String,
    overview: String,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    #[serde(default)]
    seasons: Vec<SeasonSummary>,
    #[serde(default)]
    images: ImagesDto,
}

#[derive(Deserialize)]
struct SeasonSummary {
    season_number: u32,
    poster_path: Option<String>,
}

#[derive(Deserialize)]
struct SeasonDetails {
    id: u64,
    season_number: u32,
    poster_path: Option<String>,
    #[serde(default)]
    episodes: Vec<EpisodeDetails>,
}

#[derive(Deserialize)]
struct EpisodeDetails {
    id: u64,
    name: String,
    overview: String,
    season_number: u32,
    episode_number: u32,
    runtime: Option<u64>,
    still_path: Option<String>,
    #[serde(default)]
    guest_stars: Vec<CastDto>,
    #[serde(default)]
    crew: Vec<CrewDto>,
}

#[derive(Deserialize)]
struct CastDto {
    id: u64,
    name: String,
    character: String,
}

#[derive(Deserialize)]
struct CrewDto {
    id: u64,
    name: String,
    job: String,
}

#[derive(Default, Deserialize)]
struct ImagesDto {
    #[serde(default)]
    posters: Vec<ImageDto>,
    #[serde(default)]
    backdrops: Vec<ImageDto>,
}

#[derive(Deserialize)]
struct ImageDto {
    file_path: String,
}

pub async fn search(
    config: &TmdbConfig,
    request: SearchRequest,
    http: &dyn HttpClient,
) -> Result<Vec<Candidate>, TmdbError> {
    let SearchRequest::Television {
        title,
        year,
        locales,
    } = request
    else {
        return Err(TmdbError::InvalidData(
            "TMDB television search requires a television request".to_owned(),
        ));
    };
    let mut url = config.endpoint("/3/search/tv")?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("query", &title)
            .append_pair("include_adult", "false");
        if let Some(year) = year {
            query.append_pair("first_air_date_year", &year.to_string());
        }
        if let Some(locale) = locales.first() {
            query.append_pair("language", &locale.to_string());
        }
    }
    let response: SearchResponse = get_json(config, url, http).await?;
    if response.results.is_empty() {
        return Err(TmdbError::EmptyResults);
    }
    response
        .results
        .into_iter()
        .map(|series| {
            let year = series
                .first_air_date
                .as_deref()
                .and_then(|date| date.get(..4))
                .and_then(|year| year.parse().ok());
            TelevisionCandidate::new(
                fixer_core::ProviderId::new("tmdb").map_err(data_error)?,
                ExternalId::new("tmdb", series.id.to_string()).map_err(data_error)?,
                series.name,
                year,
            )
            .map(Candidate::Television)
            .map_err(data_error)
        })
        .collect()
}

pub async fn fetch(
    config: &TmdbConfig,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<Series, TmdbError> {
    if request.media_kind != MediaKind::Television || request.external_id.namespace != "tmdb" {
        return Err(TmdbError::InvalidData(
            "TMDB fetch requires a tmdb television ID".to_owned(),
        ));
    }
    let id: u64 = request
        .external_id
        .value
        .parse()
        .map_err(|_| TmdbError::InvalidData("TMDB television ID must be numeric".to_owned()))?;
    let requested_locale = request.locales.first().map(ToString::to_string);
    let mut url = config.endpoint(&format!("/3/tv/{id}"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("append_to_response", "images,external_ids");
        if let Some(locale) = requested_locale.as_deref() {
            query.append_pair("language", locale);
        }
    }
    let details: SeriesDetails = get_json(config, url, http).await?;
    let mut series = map_series(config, &details, requested_locale.as_deref())?;
    for summary in &details.seasons {
        let mut season_url =
            config.endpoint(&format!("/3/tv/{id}/season/{}", summary.season_number))?;
        if let Some(locale) = requested_locale.as_deref() {
            season_url.query_pairs_mut().append_pair("language", locale);
        }
        let season_details: SeasonDetails = get_json(config, season_url, http).await?;
        series.seasons.push(map_season(
            config,
            season_details,
            summary.poster_path.as_deref(),
            requested_locale.as_deref(),
        )?);
    }
    series.seasons.sort_by_key(|season| season.number);
    Ok(series)
}

fn map_series(
    config: &TmdbConfig,
    details: &SeriesDetails,
    requested_locale: Option<&str>,
) -> Result<Series, TmdbError> {
    let original_language = normalized_language(&details.original_language);
    let localized = requested_locale.unwrap_or(original_language);
    let mut titles = LocalizedValue::new();
    if !details.name.trim().is_empty() {
        titles
            .insert(localized, details.name.clone())
            .map_err(data_error)?;
    }
    if original_language != localized && !details.original_name.trim().is_empty() {
        titles
            .insert(original_language, details.original_name.clone())
            .map_err(data_error)?;
    }
    let mut series = Series::new(
        WorkId::new(format!("tmdb-{}", details.id)).map_err(data_error)?,
        titles,
        OrderingScheme::Aired,
        Vec::new(),
    );
    if !details.overview.trim().is_empty() {
        series
            .summaries
            .insert(localized, details.overview.clone())
            .map_err(data_error)?;
    }
    let mut seen = BTreeSet::new();
    add_artwork(
        config,
        &mut series.artwork,
        &mut seen,
        ArtworkKind::Poster,
        details.poster_path.as_deref(),
    )?;
    add_artwork(
        config,
        &mut series.artwork,
        &mut seen,
        ArtworkKind::Backdrop,
        details.backdrop_path.as_deref(),
    )?;
    for image in &details.images.posters {
        add_artwork(
            config,
            &mut series.artwork,
            &mut seen,
            ArtworkKind::Poster,
            Some(&image.file_path),
        )?;
    }
    for image in &details.images.backdrops {
        add_artwork(
            config,
            &mut series.artwork,
            &mut seen,
            ArtworkKind::Backdrop,
            Some(&image.file_path),
        )?;
    }
    Ok(series)
}

fn map_season(
    config: &TmdbConfig,
    details: SeasonDetails,
    summary_poster: Option<&str>,
    requested_locale: Option<&str>,
) -> Result<Season, TmdbError> {
    let mut episodes = details
        .episodes
        .into_iter()
        .map(|episode| map_episode(config, episode, requested_locale))
        .collect::<Result<Vec<_>, _>>()?;
    episodes.sort_by_key(|episode| episode.sequence.episode);
    let mut season = Season::new(
        WorkId::new(format!("tmdb-season-{}", details.id)).map_err(data_error)?,
        details.season_number,
        episodes,
    )
    .map_err(data_error)?;
    let mut seen = BTreeSet::new();
    add_artwork(
        config,
        &mut season.artwork,
        &mut seen,
        ArtworkKind::Poster,
        details.poster_path.as_deref().or(summary_poster),
    )?;
    Ok(season)
}

fn map_episode(
    config: &TmdbConfig,
    details: EpisodeDetails,
    requested_locale: Option<&str>,
) -> Result<Episode, TmdbError> {
    let locale = requested_locale.unwrap_or("und");
    let mut titles = LocalizedValue::new();
    titles.insert(locale, details.name).map_err(data_error)?;
    let mut episode = Episode::new(
        WorkId::new(format!("tmdb-episode-{}", details.id)).map_err(data_error)?,
        titles,
        EpisodeSequence::aired(details.season_number, details.episode_number)
            .map_err(data_error)?,
    );
    if !details.overview.trim().is_empty() {
        episode
            .summaries
            .insert(locale, details.overview)
            .map_err(data_error)?;
    }
    episode.runtime = details
        .runtime
        .map(|minutes| Duration::from_seconds(minutes * 60));
    for cast in details.guest_stars {
        let person = Person::new(
            PersonId::new(format!("tmdb-{}", cast.id)).map_err(data_error)?,
            cast.name,
        )
        .map_err(data_error)?;
        episode.credits.push(
            Credit::new(person, CreditRole::Actor)
                .with_character(cast.character)
                .map_err(data_error)?,
        );
    }
    for crew in details.crew {
        if let Some(role) = crew_role(&crew.job) {
            let person = Person::new(
                PersonId::new(format!("tmdb-{}", crew.id)).map_err(data_error)?,
                crew.name,
            )
            .map_err(data_error)?;
            episode.credits.push(Credit::new(person, role));
        }
    }
    let mut seen = BTreeSet::new();
    add_artwork(
        config,
        &mut episode.artwork,
        &mut seen,
        ArtworkKind::Backdrop,
        details.still_path.as_deref(),
    )?;
    Ok(episode)
}

fn add_artwork(
    config: &TmdbConfig,
    artwork: &mut Vec<ArtworkReference>,
    seen: &mut BTreeSet<String>,
    kind: ArtworkKind,
    path: Option<&str>,
) -> Result<(), TmdbError> {
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    if !seen.insert(path.to_owned()) {
        return Ok(());
    }
    let id = ExternalId::new("tmdb-image", path.trim_start_matches('/').replace('/', "-"))
        .map_err(data_error)?;
    artwork.push(
        ArtworkReference::new(kind, config.image_url(path)?)
            .map_err(data_error)?
            .with_external_id(id),
    );
    Ok(())
}

async fn get_json<T: DeserializeOwned>(
    config: &TmdbConfig,
    url: url::Url,
    http: &dyn HttpClient,
) -> Result<T, TmdbError> {
    let request = HttpRequest::new(HttpMethod::Get, url.to_string()).with_header(
        Header::new("authorization", format!("Bearer {}", config.token())).map_err(data_error)?,
    );
    let response = http.execute(request).await.map_err(TmdbError::from_http)?;
    serde_json::from_slice(&response.body)
        .map_err(|error| TmdbError::MalformedResponse(error.to_string()))
}

fn normalized_language(language: &str) -> &str {
    match language {
        "cn" => "zh",
        other => other,
    }
}

fn crew_role(job: &str) -> Option<CreditRole> {
    match job {
        "Director" => Some(CreditRole::Director),
        "Screenplay" | "Writer" => Some(CreditRole::Writer),
        "Producer" | "Executive Producer" => Some(CreditRole::Producer),
        "Original Music Composer" | "Music" => Some(CreditRole::Composer),
        _ => None,
    }
}

fn data_error(error: impl std::fmt::Display) -> TmdbError {
    TmdbError::InvalidData(error.to_string())
}
