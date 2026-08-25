use crate::{AppError, AppResult};
use fixer_core::{Candidate, Movie, Resolved};
use serde::Serialize;

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
struct TitleDto {
    locale: Option<String>,
    value: String,
}
impl ResolvedMovieDto {
    pub fn from_resolved(resolved: &Resolved<Movie>) -> Self {
        let titles = resolved
            .value
            .titles
            .entries()
            .iter()
            .map(|entry| TitleDto {
                locale: entry.language().map(ToString::to_string),
                value: entry.value().clone(),
            })
            .collect::<Vec<_>>();
        let title = preferred_title(&resolved.value).to_owned();
        Self {
            schema_version: 1,
            kind: "movie",
            id: resolved.value.id.as_str().to_owned(),
            title,
            year: resolved.value.release_year(),
            titles,
            completeness: resolved.completeness,
            conflicts: resolved.conflicts.len(),
            warnings: resolved
                .warnings
                .iter()
                .map(|warning| warning.message.clone())
                .collect(),
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
pub fn resolved_text(resolved: &Resolved<Movie>) {
    let title = preferred_title(&resolved.value);
    match resolved.value.release_year() {
        Some(year) => println!("{title} ({year})"),
        None => println!("{title}"),
    }
}
fn preferred_title(movie: &Movie) -> &str {
    for locale in ["zh-CN", "en"] {
        if let Some(entry) = movie.titles.entries().iter().find(|entry| {
            entry
                .language()
                .is_some_and(|language| language.to_string() == locale)
        }) {
            return entry.value();
        }
    }
    movie
        .titles
        .entries()
        .first()
        .map(|entry| entry.value().as_str())
        .unwrap_or("<untitled>")
}

pub fn search_text(candidates: &[Candidate]) {
    for (index, candidate) in candidates.iter().enumerate() {
        if let Candidate::Movie(movie) = candidate {
            match movie.year {
                Some(year) => println!(
                    "{}\t{} ({})\t{}",
                    index + 1,
                    movie.title,
                    year,
                    movie.provider.as_str()
                ),
                None => println!(
                    "{}\t{}\t{}",
                    index + 1,
                    movie.title,
                    movie.provider.as_str()
                ),
            }
        }
    }
}
