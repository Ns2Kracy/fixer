use crate::{AppError, AppResult};
use fixer_core::{
    AnimeSeries, AnimeSeriesRelation, Candidate, LocalizedValue, Movie, MusicReleaseGroup,
    OrderingScheme, Resolved, Series,
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ResolvedAnimeDto {
    schema_version: u8,
    kind: &'static str,
    id: String,
    title: String,
    relation: AnimeSeriesRelation,
    cours: usize,
    episodes: usize,
    titles: Vec<TitleDto>,
    completeness: f32,
    conflicts: usize,
    warnings: Vec<String>,
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
    ordering: OrderingScheme,
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
            schema_version: 1,
            kind: "anime",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            relation: resolved.value.relation,
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

impl ResolvedMovieDto {
    pub fn from_resolved(resolved: &Resolved<Movie>) -> Self {
        Self {
            schema_version: 1,
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
            schema_version: 1,
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
            schema_version: 1,
            kind: "television",
            id: resolved.value.id.as_str().to_owned(),
            title: preferred_title(&resolved.value.titles).to_owned(),
            ordering: resolved.value.ordering,
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
            Candidate::Book(_) => continue,
        };
        match year {
            Some(year) => println!("{}\t{} ({})\t{}", index + 1, title, year, provider.as_str()),
            None => println!("{}\t{}\t{}", index + 1, title, provider.as_str()),
        }
    }
}
