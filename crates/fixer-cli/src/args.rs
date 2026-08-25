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
    Movie(MovieQueryArgs),
}
#[derive(Debug, Subcommand)]
pub enum ResolveCommand {
    Movie(ResolveMovieArgs),
}

#[derive(Debug, Args)]
pub struct MovieQueryArgs {
    pub title: String,
    #[arg(long)]
    pub year: Option<u16>,
}

#[derive(Debug, Args)]
pub struct ResolveMovieArgs {
    #[command(flatten)]
    pub query: MovieQueryArgs,
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
