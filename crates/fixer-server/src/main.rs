use std::{env, path::PathBuf};

use fixer_server::ServerConfig;

const DEFAULT_WEB_ROOT: &str = "web/dist";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    let web_root = env::var_os("FIXER_WEB_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_WEB_ROOT));
    fixer_server::serve(config, web_root).await?;
    Ok(())
}
