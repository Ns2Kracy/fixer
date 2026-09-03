use fixer_runtime::ConfigLoader;
use fixer_server::{ServerConfig, init_tracing};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = ConfigLoader::default().load()?;
    init_tracing(&loaded.config().logging)?;
    let runtime_config = loaded.config().clone();
    let server = ServerConfig::from_shared(&runtime_config.server)?;
    let web_root = runtime_config.server.web_root.clone();
    fixer_server::serve_configured(server, web_root, runtime_config).await?;
    Ok(())
}
