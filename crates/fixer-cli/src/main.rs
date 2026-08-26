mod args;
mod commands;
mod config;
mod render;

use args::Cli;
use clap::Parser;
use std::{fmt, process::ExitCode};

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug)]
pub(crate) struct AppError(String);
impl AppError {
    pub(crate) fn new(error: impl fmt::Display) -> Self {
        Self(error.to_string())
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for AppError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunStatus {
    Success,
    PartialSuccess,
}
impl RunStatus {
    fn exit_code(self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::PartialSuccess => ExitCode::from(3),
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match config::Config::load(&cli) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    match commands::run(cli.command, config).await {
        Ok(status) => status.exit_code(),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
