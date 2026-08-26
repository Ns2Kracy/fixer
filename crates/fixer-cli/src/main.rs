mod args;
mod commands;
mod config;
mod json;
mod render;

use args::Cli;
use clap::Parser;
use std::{fmt, process::ExitCode};

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppErrorKind {
    InvalidInput,
    Execution,
}

#[derive(Debug)]
pub(crate) struct AppError {
    message: String,
    kind: AppErrorKind,
}
impl AppError {
    pub(crate) fn new(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            kind: AppErrorKind::Execution,
        }
    }

    pub(crate) fn invalid_input(error: impl fmt::Display) -> Self {
        Self {
            message: error.to_string(),
            kind: AppErrorKind::InvalidInput,
        }
    }

    fn exit_code(&self) -> ExitCode {
        match self.kind {
            AppErrorKind::InvalidInput => ExitCode::from(2),
            AppErrorKind::Execution => ExitCode::FAILURE,
        }
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for AppError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunStatus {
    Success,
    PartialSuccess,
    ReviewRequired,
}
impl RunStatus {
    fn exit_code(self) -> ExitCode {
        match self {
            Self::Success => ExitCode::SUCCESS,
            Self::PartialSuccess => ExitCode::from(3),
            Self::ReviewRequired => ExitCode::from(4),
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let config = match config::Config::load(&cli).and_then(|config| {
        config.validate()?;
        Ok(config)
    }) {
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
            error.exit_code()
        }
    }
}
