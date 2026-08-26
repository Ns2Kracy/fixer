use crate::{MusicBrainzConfig, MusicBrainzError, provider::RequestGate};
use fixer_core::{
    AssetId, Candidate, Disc, Duration, ExternalId, FetchRequest, Header, HttpClient, HttpMethod,
    HttpRequest, LocalizedValue, MediaKind, MusicArtist, MusicCandidate, MusicRelease,
    MusicReleaseGroup, ProviderId, ReleaseId, SearchRequest, Track, TrackSequence, WorkId,
};
use serde::{Deserialize, de::DeserializeOwned};

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(rename = "release-groups", default)]
    release_groups: Vec<ReleaseGroupSummary>,
}

#[derive(Debug, Deserialize)]
struct ReleaseGroupSummary {
    id: String,
    title: String,
    #[serde(rename = "first-release-date", default)]
    first_release_date: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseGroupDetail {
    id: String,
    title: String,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<ArtistCredit>,
    #[serde(default)]
    releases: Vec<ReleaseSummary>,
}

#[derive(Debug, Deserialize)]
struct ArtistCredit {
    name: String,
    artist: ArtistSummary,
}

#[derive(Debug, Deserialize)]
struct ArtistSummary {
    id: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseSummary {
    id: String,
    #[serde(default)]
    date: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseDetail {
    id: String,
    #[serde(default)]
    media: Vec<Medium>,
}

#[derive(Debug, Deserialize)]
struct Medium {
    position: u32,
    #[serde(default)]
    tracks: Vec<TrackDto>,
}

#[derive(Debug, Deserialize)]
struct TrackDto {
    id: String,
    position: u32,
    title: String,
    #[serde(default)]
    length: Option<u64>,
    recording: Recording,
}

#[derive(Debug, Deserialize)]
struct Recording {
    id: String,
}

pub async fn search(
    config: &MusicBrainzConfig,
    gate: &RequestGate,
    request: SearchRequest,
    http: &dyn HttpClient,
) -> Result<Vec<Candidate>, MusicBrainzError> {
    let SearchRequest::Music { title, year, .. } = request else {
        return Err(MusicBrainzError::InvalidData(
            "MusicBrainz search requires a music request".to_owned(),
        ));
    };
    let mut query = format!("releasegroup:\"{}\"", escape_lucene(&title));
    if let Some(year) = year {
        query.push_str(&format!(" AND firstreleasedate:{year}"));
    }
    let mut url = config
        .base_url()
        .join("release-group/")
        .map_err(config_error)?;
    url.query_pairs_mut()
        .append_pair("query", &query)
        .append_pair("fmt", "json")
        .append_pair("limit", "25");
    let response: SearchResponse = get_json(config, gate, url, http).await?;
    response
        .release_groups
        .into_iter()
        .map(|group| {
            MusicCandidate::new(
                ProviderId::new("musicbrainz").map_err(data_error)?,
                ExternalId::new("musicbrainz-release-group", &group.id).map_err(data_error)?,
                group.title,
                parse_year(&group.first_release_date),
            )
            .map(Candidate::Music)
            .map_err(data_error)
        })
        .collect()
}

pub async fn fetch(
    config: &MusicBrainzConfig,
    gate: &RequestGate,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<MusicReleaseGroup, MusicBrainzError> {
    if request.media_kind != MediaKind::Music
        || request.external_id.namespace != "musicbrainz-release-group"
    {
        return Err(MusicBrainzError::InvalidData(
            "MusicBrainz fetch requires a musicbrainz-release-group ID".to_owned(),
        ));
    }
    let group_id = &request.external_id.value;
    let mut group_url = config
        .base_url()
        .join(&format!("release-group/{group_id}"))
        .map_err(config_error)?;
    group_url
        .query_pairs_mut()
        .append_pair("inc", "releases+artist-credits")
        .append_pair("fmt", "json");
    let group: ReleaseGroupDetail = get_json(config, gate, group_url, http).await?;
    if group.id != *group_id {
        return Err(MusicBrainzError::InvalidData(
            "MusicBrainz release-group response did not match the request".to_owned(),
        ));
    }
    let release = representative_release(&group.releases).ok_or(MusicBrainzError::NotFound)?;
    let mut release_url = config
        .base_url()
        .join(&format!("release/{}", release.id))
        .map_err(config_error)?;
    release_url
        .query_pairs_mut()
        .append_pair("inc", "recordings")
        .append_pair("fmt", "json");
    let detail: ReleaseDetail = get_json(config, gate, release_url, http).await?;
    if detail.id != release.id {
        return Err(MusicBrainzError::InvalidData(
            "MusicBrainz release response did not match the selected release".to_owned(),
        ));
    }
    map_release_group(group, detail)
}

async fn get_json<T: DeserializeOwned>(
    config: &MusicBrainzConfig,
    gate: &RequestGate,
    url: url::Url,
    http: &dyn HttpClient,
) -> Result<T, MusicBrainzError> {
    gate.wait().await?;
    let request = HttpRequest::new(HttpMethod::Get, url.to_string())
        .with_header(Header::new("accept", "application/json").map_err(data_error)?)
        .with_header(Header::new("user-agent", config.user_agent()).map_err(data_error)?);
    let response = http
        .execute(request)
        .await
        .map_err(MusicBrainzError::from_http)?;
    serde_json::from_slice(&response.body)
        .map_err(|error| MusicBrainzError::MalformedResponse(error.to_string()))
}

fn map_release_group(
    group: ReleaseGroupDetail,
    release: ReleaseDetail,
) -> Result<MusicReleaseGroup, MusicBrainzError> {
    let credit = group.artist_credit.first().ok_or_else(|| {
        MusicBrainzError::InvalidData("MusicBrainz release group has no artist credit".to_owned())
    })?;
    let artist = MusicArtist::new(
        WorkId::new(format!("musicbrainz-artist-{}", credit.artist.id)).map_err(data_error)?,
        &credit.name,
    )
    .map_err(data_error)?;
    let mut discs = release
        .media
        .into_iter()
        .map(|medium| {
            let tracks = medium
                .tracks
                .into_iter()
                .map(|track| {
                    let mut titles = LocalizedValue::new();
                    titles.insert("und", track.title).map_err(data_error)?;
                    let identity = if track.recording.id.is_empty() {
                        track.id
                    } else {
                        track.recording.id
                    };
                    Ok(Track::new(
                        AssetId::new(format!("musicbrainz-recording-{identity}"))
                            .map_err(data_error)?,
                        titles,
                        TrackSequence::new(medium.position, track.position).map_err(data_error)?,
                        Duration::from_seconds(track.length.unwrap_or_default().div_ceil(1000)),
                    ))
                })
                .collect::<Result<Vec<_>, MusicBrainzError>>()?;
            Disc::new(medium.position, tracks).map_err(data_error)
        })
        .collect::<Result<Vec<_>, _>>()?;
    discs.sort_by_key(|disc| disc.number);
    let mut titles = LocalizedValue::new();
    titles.insert("und", group.title).map_err(data_error)?;
    Ok(MusicReleaseGroup::new(
        WorkId::new(format!("musicbrainz-release-group-{}", group.id)).map_err(data_error)?,
        titles,
        artist,
        vec![MusicRelease::new(
            ReleaseId::new(format!("musicbrainz-release-{}", release.id)).map_err(data_error)?,
            discs,
        )],
    ))
}

fn representative_release(releases: &[ReleaseSummary]) -> Option<&ReleaseSummary> {
    releases.iter().min_by(|left, right| {
        let left_key = (left.date.is_empty(), left.date.as_str(), left.id.as_str());
        let right_key = (
            right.date.is_empty(),
            right.date.as_str(),
            right.id.as_str(),
        );
        left_key.cmp(&right_key)
    })
}

fn escape_lucene(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '"') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn parse_year(date: &str) -> Option<u16> {
    date.get(..4)?.parse().ok()
}

fn config_error(error: impl std::fmt::Display) -> MusicBrainzError {
    MusicBrainzError::InvalidConfig(error.to_string())
}

fn data_error(error: impl std::fmt::Display) -> MusicBrainzError {
    MusicBrainzError::InvalidData(error.to_string())
}
