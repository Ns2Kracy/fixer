use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "fixer",
    version,
    about = "Deterministic media metadata resolution"
)]
pub struct Cli {
    #[arg(long, global = true, value_name = "FILE")]
    pub config: Option<PathBuf>,
    #[arg(long, global = true, value_name = "PATH")]
    pub local_root: Option<PathBuf>,
    #[arg(long, global = true)]
    pub offline: bool,
    #[arg(long, global = true, value_name = "URL")]
    pub proxy: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Search {
        #[command(subcommand)]
        command: SearchCommand,
    },
    Resolve {
        #[command(subcommand)]
        command: ResolveCommand,
    },
    Scrape(ScrapeArgs),
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    Providers {
        #[command(subcommand)]
        command: ProvidersCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum SearchCommand {
    Anime(AnimeQueryArgs),
    Movie(MovieQueryArgs),
    Television(TelevisionQueryArgs),
}
#[derive(Debug, Subcommand)]
pub enum ResolveCommand {
    Anime(ResolveAnimeArgs),
    Movie(ResolveMovieArgs),
    Television(ResolveTelevisionArgs),
}

#[derive(Debug, Args)]
pub struct AnimeQueryArgs {
    pub title: String,
    #[arg(long)]
    pub year: Option<u16>,
    #[arg(long = "external-id", value_name = "NAMESPACE:ID")]
    pub external_ids: Vec<String>,
}

#[derive(Debug, Args)]
pub struct MovieQueryArgs {
    pub title: String,
    #[arg(long)]
    pub year: Option<u16>,
}

#[derive(Debug, Args)]
pub struct TelevisionQueryArgs {
    pub title: String,
    #[arg(long)]
    pub year: Option<u16>,
    #[arg(long = "external-id", value_name = "NAMESPACE:ID")]
    pub external_ids: Vec<String>,
    #[arg(long, value_enum)]
    pub ordering: Option<OrderingArg>,
}

#[derive(Debug, Args)]
pub struct ResolveAnimeArgs {
    #[command(flatten)]
    pub query: AnimeQueryArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ResolveMovieArgs {
    #[command(flatten)]
    pub query: MovieQueryArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ResolveTelevisionArgs {
    #[command(flatten)]
    pub query: TelevisionQueryArgs,
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ScrapeArgs {
    pub path: PathBuf,
    #[arg(long, value_enum)]
    pub kind: MediaKindArg,
    #[arg(long, conflicts_with = "apply")]
    pub dry_run: bool,
    #[arg(long, conflicts_with = "dry_run")]
    pub apply: bool,
    #[arg(long, value_enum, default_value_t = PlacementArg::InPlace)]
    pub placement: PlacementArg,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MediaKindArg {
    Movie,
    Television,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OrderingArg {
    Aired,
    Dvd,
    Absolute,
}

impl From<OrderingArg> for fixer_core::OrderingScheme {
    fn from(value: OrderingArg) -> Self {
        match value {
            OrderingArg::Aired => Self::Aired,
            OrderingArg::Dvd => Self::Dvd,
            OrderingArg::Absolute => Self::Absolute,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum PlacementArg {
    InPlace,
    Symlink,
    Hardlink,
    Copy,
    Reflink,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Validate,
}
#[derive(Debug, Subcommand)]
pub enum ProvidersCommand {
    List,
}
