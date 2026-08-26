use fixer_core::{HttpError, ProviderError};
use fixer_provider_anilist::{AniListConfig, AniListError};

#[test]
fn default_endpoint_supports_public_unauthenticated_queries() {
    let config = AniListConfig::default();

    assert_eq!(config.endpoint().as_str(), "https://graphql.anilist.co/");
    assert_eq!(config.access_token(), None);
}

#[test]
fn endpoint_and_optional_token_overrides_are_validated_and_redacted() {
    let config = AniListConfig::default()
        .with_endpoint("http://127.0.0.1:9000/graphql")
        .unwrap()
        .with_access_token("secret-token")
        .unwrap();

    assert_eq!(config.endpoint().as_str(), "http://127.0.0.1:9000/graphql");
    assert_eq!(config.access_token(), Some("secret-token"));
    assert!(!format!("{config:?}").contains("secret-token"));
    assert!(matches!(
        AniListConfig::default().with_endpoint("file:///tmp/graphql"),
        Err(AniListError::InvalidConfig(_))
    ));
    assert!(matches!(
        AniListConfig::default().with_access_token("  "),
        Err(AniListError::InvalidConfig(_))
    ));
}

#[test]
fn transport_and_graphql_failures_have_stable_codes() {
    let cases = [
        (HttpError::Offline, "offline"),
        (HttpError::Timeout, "timeout"),
        (HttpError::Status { status: 401 }, "unauthorized"),
        (HttpError::Status { status: 429 }, "rate_limited"),
        (HttpError::Status { status: 503 }, "unexpected_status"),
    ];
    for (transport, expected) in cases {
        assert_eq!(AniListError::from_http(transport).code(), expected);
    }
    assert_eq!(
        AniListError::GraphQl("bad query".to_owned()).code(),
        "graphql"
    );
    assert_eq!(
        ProviderError::from(AniListError::NotFound),
        ProviderError::NotFound
    );
}
