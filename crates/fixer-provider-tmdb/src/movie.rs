use crate::{TmdbConfig, TmdbError};
use fixer_core::{
    ArtworkKind, ArtworkReference, Candidate, Credit, CreditRole, Duration, ExternalId,
    FetchRequest, Genre, Header, HttpClient, HttpMethod, HttpRequest, LocalizedValue, MediaKind,
    Movie, MovieCandidate, MovieRelease, Person, PersonId, Rating, ReleaseDate, ReleaseId,
    SearchRequest, WorkId,
};
use serde::{Deserialize, de::DeserializeOwned};
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct SearchResponse {
    results: Vec<SearchMovie>,
}
#[derive(Deserialize)]
struct SearchMovie {
    id: u64,
    title: String,
    release_date: Option<String>,
}
#[derive(Deserialize)]
struct Details {
    id: u64,
    title: String,
    original_title: String,
    original_language: String,
    overview: String,
    release_date: Option<String>,
    runtime: Option<u64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    vote_average: Option<f32>,
    #[serde(default)]
    genres: Vec<GenreDto>,
    #[serde(default)]
    credits: CreditsDto,
    #[serde(default)]
    images: ImagesDto,
}
#[derive(Deserialize)]
struct GenreDto {
    name: String,
}
#[derive(Default, Deserialize)]
struct CreditsDto {
    #[serde(default)]
    cast: Vec<CastDto>,
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
    let SearchRequest::Movie {
        title,
        year,
        locales,
    } = request
    else {
        return Err(TmdbError::InvalidData(
            "TMDB only supports movie search".to_owned(),
        ));
    };
    let mut url = config.endpoint("/3/search/movie")?;
    {
        let mut query = url.query_pairs_mut();
        query
            .append_pair("query", &title)
            .append_pair("include_adult", "false");
        if let Some(year) = year {
            query.append_pair("primary_release_year", &year.to_string());
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
        .map(|movie| {
            let year = movie
                .release_date
                .as_deref()
                .and_then(|date| date.get(..4))
                .and_then(|year| year.parse().ok());
            MovieCandidate::new(
                fixer_core::ProviderId::new("tmdb").map_err(data_error)?,
                ExternalId::new("tmdb", movie.id.to_string()).map_err(data_error)?,
                movie.title,
                year,
            )
            .map(Candidate::Movie)
            .map_err(data_error)
        })
        .collect()
}

pub async fn fetch(
    config: &TmdbConfig,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<Movie, TmdbError> {
    if request.media_kind != MediaKind::Movie || request.external_id.namespace != "tmdb" {
        return Err(TmdbError::InvalidData(
            "TMDB fetch requires a tmdb movie ID".to_owned(),
        ));
    }
    let id: u64 = request
        .external_id
        .value
        .parse()
        .map_err(|_| TmdbError::InvalidData("TMDB movie ID must be numeric".to_owned()))?;
    let mut url = config.endpoint(&format!("/3/movie/{id}"))?;
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("append_to_response", "credits,images,external_ids");
        if let Some(locale) = request.locales.first() {
            query.append_pair("language", &locale.to_string());
        }
    }
    let details: Details = get_json(config, url, http).await?;
    map_details(
        config,
        details,
        request.locales.first().map(ToString::to_string).as_deref(),
    )
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

fn map_details(
    config: &TmdbConfig,
    details: Details,
    requested_locale: Option<&str>,
) -> Result<Movie, TmdbError> {
    let mut titles = LocalizedValue::new();
    let original_language = normalized_language(&details.original_language);
    let localized = requested_locale.unwrap_or(original_language);
    if !details.title.trim().is_empty() {
        titles
            .insert(localized, details.title)
            .map_err(data_error)?;
    }
    if original_language != localized && !details.original_title.trim().is_empty() {
        titles
            .insert(original_language, details.original_title)
            .map_err(data_error)?;
    }
    let mut movie = Movie::new(
        WorkId::new(format!("tmdb-{}", details.id)).map_err(data_error)?,
        titles,
    );
    if !details.overview.trim().is_empty() {
        movie
            .summaries
            .insert(localized, details.overview)
            .map_err(data_error)?;
    }
    if let Some(date) = details
        .release_date
        .as_deref()
        .filter(|date| !date.is_empty())
    {
        let mut release = MovieRelease::new(
            ReleaseId::new(format!("tmdb-{}-release", details.id)).map_err(data_error)?,
            parse_date(date)?,
        );
        release.runtime = details
            .runtime
            .map(|minutes| Duration::from_seconds(minutes * 60));
        movie.releases.push(release);
    }
    for value in details.genres {
        movie
            .genres
            .push(Genre::new(value.name).map_err(data_error)?);
    }
    for cast in details.credits.cast {
        let person = Person::new(
            PersonId::new(format!("tmdb-{}", cast.id)).map_err(data_error)?,
            cast.name,
        )
        .map_err(data_error)?;
        movie.credits.push(
            Credit::new(person, CreditRole::Actor)
                .with_character(cast.character)
                .map_err(data_error)?,
        );
    }
    for crew in details.credits.crew {
        if let Some(role) = crew_role(&crew.job) {
            let person = Person::new(
                PersonId::new(format!("tmdb-{}", crew.id)).map_err(data_error)?,
                crew.name,
            )
            .map_err(data_error)?;
            movie.credits.push(Credit::new(person, role));
        }
    }
    let mut images = BTreeSet::new();
    add_image(
        config,
        &mut movie,
        &mut images,
        ArtworkKind::Poster,
        details.poster_path,
    )?;
    add_image(
        config,
        &mut movie,
        &mut images,
        ArtworkKind::Backdrop,
        details.backdrop_path,
    )?;
    for image in details.images.posters {
        add_image(
            config,
            &mut movie,
            &mut images,
            ArtworkKind::Poster,
            Some(image.file_path),
        )?;
    }
    for image in details.images.backdrops {
        add_image(
            config,
            &mut movie,
            &mut images,
            ArtworkKind::Backdrop,
            Some(image.file_path),
        )?;
    }
    if let Some(rating) = details.vote_average.filter(|value| *value > 0.0) {
        movie
            .ratings
            .push(Rating::new("tmdb", rating, 10.0).map_err(data_error)?);
    }
    Ok(movie)
}
fn add_image(
    config: &TmdbConfig,
    movie: &mut Movie,
    seen: &mut BTreeSet<String>,
    kind: ArtworkKind,
    path: Option<String>,
) -> Result<(), TmdbError> {
    let Some(path) = path.filter(|path| !path.is_empty()) else {
        return Ok(());
    };
    if !seen.insert(path.clone()) {
        return Ok(());
    }
    let id = ExternalId::new("tmdb-image", path.trim_start_matches('/').replace('/', "-"))
        .map_err(data_error)?;
    movie.artwork.push(
        ArtworkReference::new(kind, config.image_url(&path)?)
            .map_err(data_error)?
            .with_external_id(id),
    );
    Ok(())
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
fn parse_date(value: &str) -> Result<ReleaseDate, TmdbError> {
    let parts = value.split('-').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(TmdbError::InvalidData(format!(
            "invalid release date `{value}`"
        )));
    }
    ReleaseDate::ymd(
        parts[0]
            .parse()
            .map_err(|_| TmdbError::InvalidData(value.to_owned()))?,
        parts[1]
            .parse()
            .map_err(|_| TmdbError::InvalidData(value.to_owned()))?,
        parts[2]
            .parse()
            .map_err(|_| TmdbError::InvalidData(value.to_owned()))?,
    )
    .map_err(data_error)
}
fn data_error(error: impl std::fmt::Display) -> TmdbError {
    TmdbError::InvalidData(error.to_string())
}
