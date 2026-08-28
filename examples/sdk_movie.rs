use fixer_provider_local::{LocalProvider, parse_nfo};
use fixer_sdk::Fixer;

const MOVIE_NFO: &str =
    include_str!("../tests/fixtures/library/movie/In the Mood for Love (2000)/movie.nfo");

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let movie = parse_nfo(MOVIE_NFO)?;
    let provider = LocalProvider::from_documents([movie])?;
    let fixer = Fixer::builder()
        .provider(provider)
        .preferred_languages(["zh-Hans", "en"])?
        .offline()
        .build()?;

    let resolved = fixer.movie("花样年华").year(2000).resolve().await?;
    println!(
        "{} ({})",
        resolved.value().titles.entries()[0].value(),
        resolved.value().release_year().unwrap_or_default()
    );
    Ok(())
}
