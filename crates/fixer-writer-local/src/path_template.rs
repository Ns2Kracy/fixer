//! Safe path templates with a bounded variable and filter surface.

use fixer_core::{CoreError, LocalePolicy, Movie, Resolved};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};
use thiserror::Error;

/// Template validation or rendering failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TemplateError {
    #[error("invalid template syntax: {0}")]
    InvalidSyntax(String),
    #[error("unsupported template variable `{0}`")]
    UnsupportedVariable(String),
    #[error("unsupported template filter `{0}`")]
    UnsupportedFilter(String),
    #[error("template variable `{0}` is missing")]
    MissingVariable(String),
    #[error("rendered output path is unsafe: `{0}`")]
    UnsafePath(String),
    #[error("invalid locale policy: {0}")]
    Locale(String),
}
impl From<CoreError> for TemplateError {
    fn from(error: CoreError) -> Self {
        Self::Locale(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expression {
    variable: String,
    filters: Vec<String>,
}

/// Values exposed to built-in movie templates.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateContext {
    values: BTreeMap<String, String>,
}
impl TemplateContext {
    /// Builds a bounded context for validating and previewing templates without writing.
    pub fn preview(
        title: impl Into<String>,
        id: impl Into<String>,
        year: Option<u16>,
        edition: Option<String>,
    ) -> Result<Self, TemplateError> {
        let title = title.into();
        let id = id.into();
        if title.is_empty() {
            return Err(TemplateError::MissingVariable("title".to_owned()));
        }
        if id.is_empty() {
            return Err(TemplateError::MissingVariable("id".to_owned()));
        }
        let mut values = BTreeMap::from([("title".to_owned(), title), ("id".to_owned(), id)]);
        if let Some(year) = year {
            values.insert("year".to_owned(), year.to_string());
        }
        if let Some(edition) = edition {
            values.insert("edition".to_owned(), edition);
        }
        Ok(Self { values })
    }

    /// Projects a resolved movie through an ordered locale preference.
    pub fn movie<I, S>(resolved: &Resolved<Movie>, languages: I) -> Result<Self, TemplateError>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let policy = LocalePolicy::new(languages)?;
        let title = resolved
            .value
            .titles
            .select(&policy)
            .ok_or_else(|| TemplateError::MissingVariable("title".to_owned()))?;
        let mut values = BTreeMap::new();
        values.insert("title".to_owned(), title.clone());
        values.insert("id".to_owned(), resolved.value.id.as_str().to_owned());
        if let Some(year) = resolved.value.release_year() {
            values.insert("year".to_owned(), year.to_string());
        }
        if let Some(edition) = resolved
            .value
            .releases
            .iter()
            .find_map(|release| release.edition.clone())
        {
            values.insert("edition".to_owned(), edition);
        }
        Ok(Self { values })
    }
    pub(crate) fn value(&self, variable: &str) -> Result<&str, TemplateError> {
        self.values
            .get(variable)
            .map(String::as_str)
            .ok_or_else(|| TemplateError::MissingVariable(variable.to_owned()))
    }
}

/// A validated relative output-path template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathTemplate {
    source: String,
    expressions: Vec<Expression>,
}
impl PathTemplate {
    /// Parses a template using only `title`, `year`, `edition`, and `id` plus documented filters.
    pub fn new(source: impl Into<String>) -> Result<Self, TemplateError> {
        let source = source.into();
        let expressions = parse_expressions(&source)?;
        Ok(Self {
            source,
            expressions,
        })
    }
    /// Renders and validates a safe relative output path.
    pub fn render(&self, context: &TemplateContext) -> Result<PathBuf, TemplateError> {
        let rendered = render_source(&self.source, &self.expressions, context)?;
        validate_relative_path(&rendered)
    }
}

pub fn parse_expressions(source: &str) -> Result<Vec<Expression>, TemplateError> {
    if source.contains('\0') {
        return Err(TemplateError::UnsafePath(source.to_owned()));
    }
    let mut expressions = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| TemplateError::InvalidSyntax(source.to_owned()))?;
        let raw = after[..end].trim();
        let mut parts = raw.split('|').map(str::trim);
        let variable = parts.next().unwrap_or_default().to_owned();
        if !matches!(variable.as_str(), "title" | "year" | "edition" | "id") {
            return Err(TemplateError::UnsupportedVariable(variable));
        }
        let filters = parts.map(str::to_owned).collect::<Vec<_>>();
        if let Some(filter) = filters
            .iter()
            .find(|filter| !matches!(filter.as_str(), "sanitize" | "lower" | "upper"))
        {
            return Err(TemplateError::UnsupportedFilter(filter.clone()));
        }
        expressions.push(Expression { variable, filters });
        rest = &after[end + 2..];
    }
    if rest.contains("}}") {
        return Err(TemplateError::InvalidSyntax(source.to_owned()));
    }
    Ok(expressions)
}

pub fn render_source(
    source: &str,
    expressions: &[Expression],
    context: &TemplateContext,
) -> Result<String, TemplateError> {
    let mut output = String::new();
    let mut rest = source;
    for expression in expressions {
        let start = rest
            .find("{{")
            .ok_or_else(|| TemplateError::InvalidSyntax(source.to_owned()))?;
        let after = &rest[start + 2..];
        let end = after
            .find("}}")
            .ok_or_else(|| TemplateError::InvalidSyntax(source.to_owned()))?;
        output.push_str(&rest[..start]);
        let mut value = context.value(&expression.variable)?.to_owned();
        for filter in &expression.filters {
            value = match filter.as_str() {
                "sanitize" => sanitize_segment(&value),
                "lower" => value.to_lowercase(),
                "upper" => value.to_uppercase(),
                _ => return Err(TemplateError::UnsupportedFilter(filter.clone())),
            };
        }
        output.push_str(&value);
        rest = &after[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*'
                )
            {
                ' '
            } else {
                character
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_relative_path(value: &str) -> Result<PathBuf, TemplateError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || value.contains('\0')
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(TemplateError::UnsafePath(value.to_owned()));
    }
    for component in path.components() {
        let Component::Normal(segment) = component else {
            continue;
        };
        let segment = segment.to_string_lossy();
        if segment.is_empty()
            || segment.ends_with([' ', '.'])
            || segment.chars().any(|character| {
                character.is_control()
                    || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            })
        {
            return Err(TemplateError::UnsafePath(value.to_owned()));
        }
    }
    Ok(path.to_path_buf())
}
