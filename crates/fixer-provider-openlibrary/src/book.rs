use crate::{OpenLibraryConfig, OpenLibraryError};
use fixer_core::{
    Asset, AssetId, AssetKind, BookCandidate, BookEdition, BookWork, Candidate, Credit, CreditRole,
    ExternalId, FetchRequest, Header, HttpClient, HttpMethod, HttpRequest, Isbn10, Isbn13,
    LocalizedValue, MediaKind, Person, PersonId, ProviderId, ReleaseId, SearchRequest, SourcePath,
    WorkId,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;

const SEARCH_FIELDS: &str = "key,title,first_publish_year,edition_key,isbn";

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    docs: Vec<SearchDocument>,
}

#[derive(Debug, Deserialize)]
struct SearchDocument {
    title: String,
    #[serde(default)]
    first_publish_year: Option<u16>,
    #[serde(default)]
    edition_key: Vec<String>,
    #[serde(default)]
    isbn: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EditionDto {
    key: String,
    title: String,
    #[serde(default)]
    publishers: Vec<String>,
    #[serde(default)]
    isbn_10: Vec<String>,
    #[serde(default)]
    isbn_13: Vec<String>,
    #[serde(default)]
    works: Vec<KeyRef>,
    #[serde(default)]
    covers: Vec<i64>,
}

#[derive(Debug, Deserialize)]
struct WorkDto {
    key: String,
    title: String,
    #[serde(default)]
    authors: Vec<AuthorRef>,
}

#[derive(Debug, Deserialize)]
struct AuthorRef {
    author: KeyRef,
}

#[derive(Debug, Deserialize)]
struct AuthorDto {
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct KeyRef {
    key: String,
}

pub async fn search(
    config: &OpenLibraryConfig,
    request: SearchRequest,
    http: &dyn HttpClient,
) -> Result<Vec<Candidate>, OpenLibraryError> {
    let SearchRequest::Book { title, year, .. } = request else {
        return Err(OpenLibraryError::InvalidData(
            "Open Library search requires a book request".to_owned(),
        ));
    };
    let mut url = config
        .api_base_url()
        .join("search.json")
        .map_err(config_error)?;
    url.query_pairs_mut()
        .append_pair("title", &title)
        .append_pair("fields", SEARCH_FIELDS)
        .append_pair("limit", "25");
    let response: SearchResponse = get_json(config, url, http).await?;
    response
        .docs
        .into_iter()
        .filter_map(|document| map_candidate(document, year).transpose())
        .collect()
}

pub async fn fetch(
    config: &OpenLibraryConfig,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<BookWork, OpenLibraryError> {
    if request.media_kind != MediaKind::Book {
        return Err(OpenLibraryError::InvalidData(
            "Open Library fetch requires a book request".to_owned(),
        ));
    }
    let edition_path = match request.external_id.namespace.as_str() {
        "isbn" => format!("isbn/{}.json", request.external_id.value),
        "openlibrary-edition" => format!("books/{}.json", request.external_id.value),
        _ => {
            return Err(OpenLibraryError::InvalidData(
                "Open Library fetch requires an isbn or openlibrary-edition ID".to_owned(),
            ));
        }
    };
    let edition: EditionDto = get_json(
        config,
        config
            .api_base_url()
            .join(&edition_path)
            .map_err(config_error)?,
        http,
    )
    .await?;
    let work_key = edition
        .works
        .first()
        .map(|reference| reference.key.as_str())
        .ok_or_else(|| {
            OpenLibraryError::InvalidData("Open Library edition has no work reference".to_owned())
        })?;
    let work: WorkDto = get_json(
        config,
        config
            .api_base_url()
            .join(&json_path(work_key))
            .map_err(config_error)?,
        http,
    )
    .await?;
    let mut author_keys = BTreeSet::new();
    for reference in &work.authors {
        author_keys.insert(reference.author.key.clone());
    }
    let mut authors = Vec::with_capacity(author_keys.len());
    for key in author_keys {
        let author: AuthorDto = get_json(
            config,
            config
                .api_base_url()
                .join(&json_path(&key))
                .map_err(config_error)?,
            http,
        )
        .await?;
        authors.push(author);
    }
    map_book(config, edition, work, authors)
}

async fn get_json<T: DeserializeOwned>(
    config: &OpenLibraryConfig,
    url: url::Url,
    http: &dyn HttpClient,
) -> Result<T, OpenLibraryError> {
    let request = HttpRequest::new(HttpMethod::Get, url.to_string())
        .with_header(Header::new("accept", "application/json").map_err(data_error)?)
        .with_header(Header::new("user-agent", config.user_agent()).map_err(data_error)?);
    let response = http
        .execute(request)
        .await
        .map_err(OpenLibraryError::from_http)?;
    serde_json::from_slice(&response.body)
        .map_err(|error| OpenLibraryError::MalformedResponse(error.to_string()))
}

fn map_candidate(
    document: SearchDocument,
    requested_year: Option<u16>,
) -> Result<Option<Candidate>, OpenLibraryError> {
    let identity = document
        .isbn
        .iter()
        .find_map(|value| {
            Isbn13::new(value.clone())
                .ok()
                .map(|isbn| isbn.as_str().to_owned())
        })
        .or_else(|| {
            document.isbn.iter().find_map(|value| {
                Isbn10::new(value.clone())
                    .ok()
                    .map(|isbn| isbn.as_str().to_owned())
            })
        })
        .map(|isbn| ExternalId::new("isbn", isbn))
        .or_else(|| {
            document
                .edition_key
                .first()
                .map(|edition| ExternalId::new("openlibrary-edition", edition))
        });
    let Some(identity) = identity else {
        return Ok(None);
    };
    BookCandidate::new(
        ProviderId::new("openlibrary").map_err(data_error)?,
        identity.map_err(data_error)?,
        document.title,
        document.first_publish_year.or(requested_year),
    )
    .map(Candidate::Book)
    .map(Some)
    .map_err(data_error)
}

fn map_book(
    config: &OpenLibraryConfig,
    edition: EditionDto,
    work: WorkDto,
    authors: Vec<AuthorDto>,
) -> Result<BookWork, OpenLibraryError> {
    let work_id = key_id(&work.key, "works")?;
    let edition_id = key_id(&edition.key, "books")?;
    let edition_title = edition.title.clone();
    let isbn_10 = edition
        .isbn_10
        .into_iter()
        .find_map(|value| Isbn10::new(value).ok())
        .ok_or_else(|| OpenLibraryError::InvalidData("edition has no valid ISBN-10".to_owned()))?;
    let isbn_13 = edition
        .isbn_13
        .into_iter()
        .find_map(|value| Isbn13::new(value).ok())
        .ok_or_else(|| OpenLibraryError::InvalidData("edition has no valid ISBN-13".to_owned()))?;
    let publisher = edition
        .publishers
        .into_iter()
        .find(|value| !value.trim().is_empty())
        .ok_or_else(|| OpenLibraryError::InvalidData("edition has no publisher".to_owned()))?;
    let assets = edition
        .covers
        .into_iter()
        .map(|cover| {
            let location = config
                .cover_base_url()
                .join(&format!("id/{cover}-L.jpg"))
                .map_err(config_error)?;
            Ok(Asset::new(
                AssetId::new(format!("openlibrary-cover-{cover}")).map_err(data_error)?,
                SourcePath::new(location.to_string()).map_err(data_error)?,
                AssetKind::Artwork,
            ))
        })
        .collect::<Result<Vec<_>, OpenLibraryError>>()?;
    let edition = BookEdition::new(
        ReleaseId::new(format!("openlibrary-edition-{edition_id}")).map_err(data_error)?,
        isbn_10,
        isbn_13,
        publisher,
        assets,
    )
    .map_err(data_error)?;
    let contributors = authors
        .into_iter()
        .map(|author| {
            let author_id = key_id(&author.key, "authors")?;
            let person = Person::new(
                PersonId::new(format!("openlibrary-author-{author_id}")).map_err(data_error)?,
                author.name,
            )
            .map_err(data_error)?;
            Ok(Credit::new(person, CreditRole::Author))
        })
        .collect::<Result<Vec<_>, OpenLibraryError>>()?;
    let mut titles = LocalizedValue::new();
    let title = if work.title.trim().is_empty() {
        edition_title
    } else {
        work.title
    };
    titles.insert("und", title).map_err(data_error)?;
    Ok(BookWork::new(
        WorkId::new(format!("openlibrary-work-{work_id}")).map_err(data_error)?,
        titles,
        contributors,
        vec![edition],
    ))
}

fn key_id<'a>(key: &'a str, collection: &str) -> Result<&'a str, OpenLibraryError> {
    key.strip_prefix(&format!("/{collection}/"))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            OpenLibraryError::InvalidData(format!("invalid Open Library {collection} key `{key}`"))
        })
}

fn json_path(key: &str) -> String {
    format!("{}.json", key.trim_start_matches('/'))
}

fn config_error(error: impl std::fmt::Display) -> OpenLibraryError {
    OpenLibraryError::InvalidConfig(error.to_string())
}

fn data_error(error: impl std::fmt::Display) -> OpenLibraryError {
    OpenLibraryError::InvalidData(error.to_string())
}
