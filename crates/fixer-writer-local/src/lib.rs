//! Planning-only local metadata writers and safe templates.

#![forbid(unsafe_code)]

mod anime;
mod book;
mod content_template;
mod episode_path;
mod json;
mod manifest;
mod music;
mod nfo;
mod organize;
mod path_template;
mod television;

pub use anime::AnimeWriter;
pub use book::BookWriter;
pub use content_template::ContentTemplate;
pub use json::JsonWriter;
pub use manifest::ManifestWriter;
pub use music::MusicWriter;
pub use nfo::NfoWriter;
pub use organize::{
    OrganizationError, OrganizationMedia, OrganizationPlacement, OrganizationRequest,
    metadata_only, organize,
};
pub use path_template::{PathTemplate, TemplateContext, TemplateError};
pub use television::TelevisionWriter;
