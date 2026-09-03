use fixer_runtime::ConfigLoader;
use fixer_server::{ServerConfig, init_tracing};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let loaded = ConfigLoader::default().load()?;
    init_tracing(&loaded.config().logging)?;
    let server = ServerConfig::from_shared(&loaded.config().server)?;
    fixer_server::serve(server, &loaded.config().server.web_root).await?;
    Ok(())
}
