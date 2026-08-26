use fixer_server::ServerConfig;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_env()?;
    fixer_server::serve(config).await?;
    Ok(())
}
