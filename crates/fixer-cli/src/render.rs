use crate::{AppError, AppResult, json::SCHEMA_VERSION};
use fixer_core::{
    AnimeSeries, AnimeSeriesRelation, BookWork, Candidate, CreditRole, LocalizedValue, Movie,
    MusicReleaseGroup, OrderingScheme, Resolved, Series,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResolvedAnimeDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    relation: &'static str,
    cours: usize,
    episodes: usize,
    titles: Vec<TitleDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ResolvedBookDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    contributors: Vec<BookContributorDto>,
    editions: Vec<BookEditionDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct BookContributorDto {
    id: String,
    name: String,
    role: &'static str,
}

#[derive(Serialize)]
struct BookEditionDto {
    id: String,
    isbn_10: String,
    isbn_13: String,
    publisher: String,
    assets: usize,
}

#[derive(Serialize)]
pub struct ResolvedMovieDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    year: Option<u16>,
    titles: Vec<TitleDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct ResolvedMusicDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    artist: String,
    releases: Vec<MusicReleaseDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct MusicReleaseDto {
    id: String,
    discs: Vec<MusicDiscDto>,
}

#[derive(Serialize)]
struct MusicDiscDto {
    number: u32,
    tracks: Vec<MusicTrackDto>,
}

#[derive(Serialize)]
struct MusicTrackDto {
    id: String,
    title: String,
    disc: u32,
    track: u32,
    duration_seconds: u64,
}

#[derive(Serialize)]
pub struct ResolvedTelevisionDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    ordering: &'static str,
    seasons: usize,
    episodes: usize,
    titles: Vec<TitleDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct TitleDto {
    locale: Option<String>,
    value: String,
}

impl ResolvedAnimeDto {
    pub fn from_resolved(resolved: &Resolved<AnimeSeries>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "anime",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            relation: anime_relation(resolved.value.relation),
            cours: resolved.value.cours.len(),
            episodes: resolved
                .value
                .cours
                .iter()
                .map(|cour| cour.episodes.len())
                .sum(),
            titles: title_dtos(&resolved.value.titles),
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: warning_messages(resolved),
        }
    }
}

impl ResolvedBookDto {
    pub fn from_resolved(resolved: &Resolved<BookWork>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "book",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            contributors: resolved
                .value
                .contributors
                .iter()
                .map(|credit| BookContributorDto {
                    id: credit.person.id.as_str().to_owned(),
                    name: credit.person.name.clone(),
                    role: credit_role(&credit.role),
                })
                .collect(),
            editions: resolved
                .value
                .editions
                .iter()
                .map(|edition| BookEditionDto {
                    id: edition.id.as_str().to_owned(),
                    isbn_10: edition.isbn_10.as_str().to_owned(),
                    isbn_13: edition.isbn_13.as_str().to_owned(),
                    publisher: edition.publisher.clone(),
                    assets: edition.assets.len(),
                })
                .collect(),
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: warning_messages(resolved),
        }
    }
}

impl ResolvedMovieDto {
    pub fn from_resolved(resolved: &Resolved<Movie>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "movie",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            year: resolved.value.release_year(),
            titles: title_dtos(&resolved.value.titles),
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: warning_messages(resolved),
        }
    }
}

impl ResolvedMusicDto {
    pub fn from_resolved(resolved: &Resolved<MusicReleaseGroup>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "music",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            artist: resolved.value.artist.name.clone(),
            releases: resolved
                .value
                .releases
                .iter()
                .map(|release| MusicReleaseDto {
                    id: release.id.as_str().to_owned(),
                    discs: release
                        .discs
                        .iter()
                        .map(|disc| MusicDiscDto {
                            number: disc.number,
                            tracks: disc
                                .tracks
                                .iter()
                                .map(|track| MusicTrackDto {
                                    id: track.id.as_str().to_owned(),
                                    title: preferred_title(&track.titles).to_owned(),
                                    disc: track.sequence.disc,
                                    track: track.sequence.track,
                                    duration_seconds: track.duration.as_seconds(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: warning_messages(resolved),
        }
    }
}

impl ResolvedTelevisionDto {
    pub fn from_resolved(resolved: &Resolved<Series>) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            kind: "television",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            ordering: ordering_scheme(resolved.value.ordering),
            seasons: resolved.value.seasons.len(),
            episodes: resolved
                .value
                .seasons
                .iter()
                .map(|season| season.episodes.len())
                .sum(),
            titles: title_dtos(&resolved.value.titles),
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: warning_messages(resolved),
        }
    }
}

pub fn json(value: &impl Serialize) -> AppResult<()> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(AppError::new)?
    );
    Ok(())
}

pub fn resolved_anime_text(resolved: &Resolved<AnimeSeries>) {
    let title = preferred_title(&resolved.value.titles);
    let episodes = resolved
        .value
        .cours
        .iter()
        .map(|cour| cour.episodes.len())
        .sum::<usize>();
    println!(
        "{title}\t{} cour(s)\t{episodes} episode(s)\t{:?}",
        resolved.value.cours.len(),
        resolved.value.relation
    );
}

pub fn resolved_book_text(resolved: &Resolved<BookWork>) {
    let title = preferred_title(&resolved.value.titles);
    let editions = resolved.value.editions.len();
    let isbn = resolved
        .value
        .editions
        .first()
        .map(|edition| edition.isbn_13.as_str())
        .unwrap_or("no-isbn");
    println!("{title}\\t{editions} edition(s)\\t{isbn}");
}

pub fn resolved_movie_text(resolved: &Resolved<Movie>) {
    let title = preferred_title(&resolved.value.titles);
    match resolved.value.release_year() {
        Some(year) => println!("{title} ({year})"),
        None => println!("{title}"),
    }
}

pub fn resolved_music_text(resolved: &Resolved<MusicReleaseGroup>) {
    let title = preferred_title(&resolved.value.titles);
    let discs = resolved
        .value
        .releases
        .iter()
        .map(|release| release.discs.len())
        .sum::<usize>();
    let tracks = resolved
        .value
        .releases
        .iter()
        .flat_map(|release| &release.discs)
        .map(|disc| disc.tracks.len())
        .sum::<usize>();
    println!(
        "{} — {title}\t{discs} disc(s)\t{tracks} track(s)",
        resolved.value.artist.name
    );
}

pub fn resolved_television_text(resolved: &Resolved<Series>) {
    let title = preferred_title(&resolved.value.titles);
    let episodes = resolved
        .value
        .seasons
        .iter()
        .map(|season| season.episodes.len())
        .sum::<usize>();
    println!(
        "{title}\t{} season(s)\t{episodes} episode(s)\t{:?}",
        resolved.value.seasons.len(),
        resolved.value.ordering
    );
}

const fn anime_relation(relation: AnimeSeriesRelation) -> &'static str {
    match relation {
        AnimeSeriesRelation::Original => "original",
        AnimeSeriesRelation::Adaptation => "adaptation",
        AnimeSeriesRelation::Sequel => "sequel",
        AnimeSeriesRelation::Prequel => "prequel",
        AnimeSeriesRelation::SideStory => "side_story",
        AnimeSeriesRelation::SpinOff => "spin_off",
    }
}

const fn credit_role(role: &CreditRole) -> &'static str {
    match role {
        CreditRole::Director => "director",
        CreditRole::Writer => "writer",
        CreditRole::Actor => "actor",
        CreditRole::Author => "author",
        CreditRole::Editor => "editor",
        CreditRole::Translator => "translator",
        CreditRole::Performer => "performer",
        CreditRole::Composer => "composer",
        CreditRole::Producer => "producer",
    }
}

const fn ordering_scheme(ordering: OrderingScheme) -> &'static str {
    match ordering {
        OrderingScheme::Aired => "aired",
        OrderingScheme::Dvd => "dvd",
        OrderingScheme::Absolute => "absolute",
    }
}

fn title_dtos(titles: &LocalizedValue<String>) -> Vec<TitleDto> {
    titles
        .entries()
        .iter()
        .map(|entry| TitleDto {
            locale: entry.language().map(ToString::to_string),
            value: entry.value().clone(),
        })
        .collect()
}

fn warning_messages<T>(resolved: &Resolved<T>) -> Vec<String> {
    resolved
        .warnings
        .iter()
        .map(|warning| warning.message.clone())
        .collect()
}

fn preferred_title(titles: &LocalizedValue<String>) -> &str {
    for locale in ["zh-CN", "en", "und"] {
        if let Some(entry) = titles.entries().iter().find(|entry| {
            entry
                .language()
                .is_some_and(|language| language.to_string() == locale)
        }) {
            return entry.value();
        }
    }
    titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .unwrap_or("<untitled>")
}

pub fn search_text(candidates: &[Candidate]) {
    for (index, candidate) in candidates.iter().enumerate() {
        let (title, year, provider) = match candidate {
            Candidate::Movie(value) => (&value.title, value.year, &value.provider),
            Candidate::Television(value) => (&value.title, value.year, &value.provider),
            Candidate::Anime(value) => (&value.title, value.year, &value.provider),
            Candidate::Music(value) => (&value.title, value.year, &value.provider),
            Candidate::Book(value) => {
                match value.year {
                    Some(year) => println!(
                        "{}\t{} ({})\t{}\t{}:{}",
                        index + 1,
                        value.title,
                        year,
                        value.provider.as_str(),
                        value.external_id.namespace,
                        value.external_id.value
                    ),
                    None => println!(
                        "{}\t{}\t{}\t{}:{}",
                        index + 1,
                        value.title,
                        value.provider.as_str(),
                        value.external_id.namespace,
                        value.external_id.value
                    ),
                }
                continue;
            }
        };
        match year {
            Some(year) => println!("{}\t{} ({})\t{}", index + 1, title, year, provider.as_str()),
            None => println!("{}\t{}\t{}", index + 1, title, provider.as_str()),
        }
    }
}
