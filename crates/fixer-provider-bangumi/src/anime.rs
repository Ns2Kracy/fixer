use crate::{BangumiConfig, BangumiError};
use fixer_core::{
    AnimeCandidate, AnimeEpisode, AnimeEpisodeClass, AnimeSeries, AnimeSeriesRelation, Candidate,
    Cour, ExternalId, FetchRequest, Header, HttpClient, HttpMethod, HttpRequest, LocalizedValue,
    MediaKind, ProviderId, SearchRequest, WorkId,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const PAGE_SIZE: usize = 200;
const ANIME_SUBJECT_TYPE: u8 = 2;

#[derive(Debug, Serialize)]
struct SearchBody<'a> {
    keyword: &'a str,
    sort: &'static str,
    filter: SearchFilter,
}

#[derive(Debug, Serialize)]
struct SearchFilter {
    #[serde(rename = "type")]
    subject_types: [u8; 1],
    #[serde(skip_serializing_if = "Option::is_none")]
    air_date: Option<[String; 2]>,
}

#[derive(Debug, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
struct Page<T> {
    #[serde(default)]
    total: usize,
    #[serde(default)]
    data: Vec<T>,
}

#[derive(Debug, Deserialize)]
struct SearchSubject {
    id: u64,
    #[serde(rename = "type")]
    subject_type: u8,
    name: String,
    #[serde(default)]
    name_cn: String,
    #[serde(default, alias = "air_date")]
    date: String,
}

#[derive(Debug, Deserialize)]
struct Subject {
    id: u64,
    #[serde(rename = "type")]
    subject_type: u8,
    name: String,
    #[serde(default)]
    name_cn: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    infobox: Vec<InfoboxItem>,
}

#[derive(Debug, Deserialize)]
struct InfoboxItem {
    key: String,
    value: InfoboxValue,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum InfoboxValue {
    Text(String),
    Values(Vec<InfoboxPart>),
}

#[derive(Debug, Deserialize)]
struct InfoboxPart {
    #[serde(default)]
    k: String,
    v: String,
}

#[derive(Debug, Deserialize)]
struct Episode {
    id: u64,
    #[serde(rename = "type")]
    episode_type: u8,
    #[serde(default)]
    name: String,
    #[serde(default)]
    name_cn: String,
    sort: f64,
    #[serde(default)]
    ep: Option<f64>,
}

pub async fn search(
    config: &BangumiConfig,
    request: SearchRequest,
    http: &dyn HttpClient,
) -> Result<Vec<Candidate>, BangumiError> {
    let SearchRequest::Anime { title, year, .. } = request else {
        return Err(BangumiError::InvalidData(
            "Bangumi search requires an anime request".to_owned(),
        ));
    };
    let mut url = config.endpoint("/v0/search/subjects")?;
    url.query_pairs_mut()
        .append_pair("limit", "25")
        .append_pair("offset", "0");
    let air_date = year.map(|year| {
        [
            format!(">={year:04}-01-01"),
            format!("<{:04}-01-01", u32::from(year) + 1),
        ]
    });
    let body = serde_json::to_vec(&SearchBody {
        keyword: &title,
        sort: "match",
        filter: SearchFilter {
            subject_types: [ANIME_SUBJECT_TYPE],
            air_date,
        },
    })
    .map_err(|error| BangumiError::InvalidData(error.to_string()))?;
    let page: Page<SearchSubject> = request_json(
        config,
        HttpRequest::new(HttpMethod::Post, url.to_string()).with_body(body),
        true,
        http,
    )
    .await?;

    page.data
        .into_iter()
        .filter(|subject| subject.subject_type == ANIME_SUBJECT_TYPE)
        .map(|subject| {
            let candidate_title = best_candidate_title(&title, &subject.name, &subject.name_cn);
            AnimeCandidate::new(
                ProviderId::new("bangumi").map_err(data_error)?,
                ExternalId::new("bangumi", subject.id.to_string()).map_err(data_error)?,
                candidate_title,
                parse_year(&subject.date),
            )
            .map(Candidate::Anime)
            .map_err(data_error)
        })
        .collect()
}

pub async fn fetch(
    config: &BangumiConfig,
    request: FetchRequest,
    http: &dyn HttpClient,
) -> Result<AnimeSeries, BangumiError> {
    if request.media_kind != MediaKind::Anime || request.external_id.namespace != "bangumi" {
        return Err(BangumiError::InvalidData(
            "Bangumi fetch requires a numeric bangumi anime ID".to_owned(),
        ));
    }
    let id: u64 =
        request.external_id.value.parse().map_err(|_| {
            BangumiError::InvalidData("Bangumi anime ID must be numeric".to_owned())
        })?;
    let subject: Subject = get_json(
        config,
        config.endpoint(&format!("/v0/subjects/{id}"))?,
        http,
    )
    .await?;
    if subject.id != id || subject.subject_type != ANIME_SUBJECT_TYPE {
        return Err(BangumiError::InvalidData(
            "Bangumi subject response did not match the requested anime".to_owned(),
        ));
    }
    let episodes = fetch_episodes(config, id, http).await?;
    map_subject(subject, episodes)
}

async fn fetch_episodes(
    config: &BangumiConfig,
    subject_id: u64,
    http: &dyn HttpClient,
) -> Result<Vec<Episode>, BangumiError> {
    let mut episodes = Vec::new();
    let mut offset = 0usize;
    loop {
        let mut url = config.endpoint("/v0/episodes")?;
        url.query_pairs_mut()
            .append_pair("subject_id", &subject_id.to_string())
            .append_pair("limit", &PAGE_SIZE.to_string())
            .append_pair("offset", &offset.to_string());
        let mut page: Page<Episode> = get_json(config, url, http).await?;
        let fetched = page.data.len();
        episodes.append(&mut page.data);
        offset += fetched;
        if fetched == 0 || fetched < PAGE_SIZE || offset >= page.total {
            break;
        }
    }
    Ok(episodes)
}

async fn get_json<T: DeserializeOwned>(
    config: &BangumiConfig,
    url: url::Url,
    http: &dyn HttpClient,
) -> Result<T, BangumiError> {
    request_json(
        config,
        HttpRequest::new(HttpMethod::Get, url.to_string()),
        false,
        http,
    )
    .await
}

async fn request_json<T: DeserializeOwned>(
    config: &BangumiConfig,
    request: HttpRequest,
    has_json_body: bool,
    http: &dyn HttpClient,
) -> Result<T, BangumiError> {
    let mut request = request
        .with_header(Header::new("user-agent", config.user_agent()).map_err(data_error)?)
        .with_header(Header::new("accept", "application/json").map_err(data_error)?);
    if has_json_body {
        request = request
            .with_header(Header::new("content-type", "application/json").map_err(data_error)?);
    }
    let response = http
        .execute(request)
        .await
        .map_err(BangumiError::from_http)?;
    serde_json::from_slice(&response.body)
        .map_err(|error| BangumiError::MalformedResponse(error.to_string()))
}

fn map_subject(subject: Subject, episodes: Vec<Episode>) -> Result<AnimeSeries, BangumiError> {
    let mut titles = LocalizedValue::new();
    insert_title(&mut titles, infer_name_locale(&subject.name), subject.name)?;
    insert_title(&mut titles, "zh-Hans", subject.name_cn)?;
    for item in subject.infobox {
        add_infobox_titles(&mut titles, item)?;
    }

    let platform_class = platform_episode_class(&subject.platform);
    let episodes = episodes
        .into_iter()
        .filter_map(|episode| match map_episode(episode, platform_class) {
            Ok(Some(episode)) => Some(Ok(episode)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut anime = AnimeSeries::new(
        WorkId::new(format!("bangumi-{}", subject.id)).map_err(data_error)?,
        titles,
        AnimeSeriesRelation::Original,
        vec![Cour::new(1, episodes).map_err(data_error)?],
    );
    if !subject.summary.trim().is_empty() {
        anime
            .summaries
            .insert("zh-Hans", subject.summary)
            .map_err(data_error)?;
    }
    Ok(anime)
}

fn map_episode(
    episode: Episode,
    platform_class: AnimeEpisodeClass,
) -> Result<Option<AnimeEpisode>, BangumiError> {
    let class = match episode.episode_type {
        0 => platform_class,
        1 => AnimeEpisodeClass::Special,
        _ => return Ok(None),
    };
    let number = if episode.episode_type == 0 {
        positive_integer(episode.ep).or_else(|| positive_integer(Some(episode.sort)))
    } else {
        positive_integer(Some(episode.sort)).or_else(|| positive_integer(episode.ep))
    }
    .ok_or_else(|| {
        BangumiError::InvalidData(format!(
            "Bangumi episode {} has no positive integral number",
            episode.id
        ))
    })?;
    let mut titles = LocalizedValue::new();
    insert_title(&mut titles, infer_name_locale(&episode.name), episode.name)?;
    insert_title(&mut titles, "zh-Hans", episode.name_cn)?;
    AnimeEpisode::new(
        WorkId::new(format!("bangumi-episode-{}", episode.id)).map_err(data_error)?,
        titles,
        class,
        Some(number),
        None,
    )
    .map(Some)
    .map_err(data_error)
}

fn add_infobox_titles(
    titles: &mut LocalizedValue<String>,
    item: InfoboxItem,
) -> Result<(), BangumiError> {
    if let Some(locale) = locale_for_label(&item.key) {
        match item.value {
            InfoboxValue::Text(value) => insert_title(titles, locale, value)?,
            InfoboxValue::Values(values) => {
                for value in values {
                    insert_title(
                        titles,
                        locale_for_label(&value.k).unwrap_or(locale),
                        value.v,
                    )?;
                }
            }
        }
        return Ok(());
    }
    if !is_alias_key(&item.key) {
        return Ok(());
    }
    match item.value {
        InfoboxValue::Text(value) => {
            let locale = infer_name_locale(&value);
            insert_title(titles, locale, value)?;
        }
        InfoboxValue::Values(values) => {
            for value in values {
                let locale =
                    locale_for_label(&value.k).unwrap_or_else(|| infer_name_locale(&value.v));
                insert_title(titles, locale, value.v)?;
            }
        }
    }
    Ok(())
}

fn insert_title(
    titles: &mut LocalizedValue<String>,
    locale: &str,
    value: String,
) -> Result<(), BangumiError> {
    let value = value.trim().to_owned();
    if value.is_empty()
        || titles.entries().iter().any(|entry| {
            entry
                .language()
                .is_some_and(|language| language.normalized() == locale.to_ascii_lowercase())
                && entry.value() == &value
        })
    {
        return Ok(());
    }
    titles.insert(locale, value).map_err(data_error)
}

fn best_candidate_title(query: &str, original: &str, chinese: &str) -> String {
    if normalize(query) == normalize(chinese) && !chinese.trim().is_empty() {
        chinese.to_owned()
    } else {
        original.to_owned()
    }
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<String>().to_lowercase()
}

fn parse_year(date: &str) -> Option<u16> {
    date.get(..4)?.parse().ok()
}

fn positive_integer(value: Option<f64>) -> Option<u32> {
    let value = value?;
    (value.is_finite()
        && value > 0.0
        && value <= f64::from(u32::MAX)
        && value.fract().abs() < f64::EPSILON)
        .then_some(value as u32)
}

fn platform_episode_class(platform: &str) -> AnimeEpisodeClass {
    match platform.trim().to_ascii_lowercase().as_str() {
        "ova" => AnimeEpisodeClass::Ova,
        "ona" | "web" => AnimeEpisodeClass::Ona,
        _ => AnimeEpisodeClass::Regular,
    }
}

fn is_alias_key(key: &str) -> bool {
    matches!(key.trim(), "别名" | "別名" | "Alias" | "Aliases")
}

fn locale_for_label(label: &str) -> Option<&'static str> {
    let label = label.trim().to_ascii_lowercase();
    if label.contains("繁体") || label.contains("繁體") || label.contains("traditional") {
        Some("zh-Hant")
    } else if label.contains("简体")
        || label.contains("簡體")
        || label == "中文名"
        || label.contains("simplified")
    {
        Some("zh-Hans")
    } else if label.contains("英文") || label.contains("english") {
        Some("en")
    } else if label.contains("日文") || label.contains("日本語") || label.contains("japanese")
    {
        Some("ja")
    } else {
        None
    }
}

fn infer_name_locale(value: &str) -> &'static str {
    if value.chars().any(|character| {
        ('\u{3040}'..='\u{30ff}').contains(&character)
            || ('\u{31f0}'..='\u{31ff}').contains(&character)
    }) {
        "ja"
    } else if value.is_ascii()
        && value
            .chars()
            .any(|character| character.is_ascii_alphabetic())
    {
        "en"
    } else {
        "ja"
    }
}

fn data_error(error: impl std::fmt::Display) -> BangumiError {
    BangumiError::InvalidData(error.to_string())
}
