use fixer_runtime::ConfigLoader;
use fixer_server::{ServerConfig, init_tracing};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = ConfigLoader::default().load()?;
    init_tracing(&loaded.config().logging)?;
    let server = ServerConfig::from_shared(&loaded.config().server)?;
    let web_root = loaded.config().server.web_root.clone();
    fixer_server::serve_with_config_handle(server, web_root, loaded.into_handle()).await?;
    Ok(())
}
