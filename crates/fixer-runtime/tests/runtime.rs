use fixer_core::Movie;
use fixer_provider_local::LocalProvider;
use fixer_runtime::{FixerConfig, SecretString, build_fixer};

#[test]
fn builds_every_configured_provider_through_the_shared_boundary() {
    let local = LocalProvider::from_documents(Vec::<Movie>::new()).unwrap();
    let mut config = FixerConfig {
        enabled_providers: vec![
            "local".to_owned(),
            "tmdb".to_owned(),
            "bangumi".to_owned(),
            "musicbrainz".to_owned(),
            "openlibrary".to_owned(),
            "anilist".to_owned(),
        ],
        offline: true,
        ..FixerConfig::default()
    };
    config.providers.tmdb.api_token = Some(SecretString::new("test-token"));

    build_fixer(&config, local).unwrap();
}
