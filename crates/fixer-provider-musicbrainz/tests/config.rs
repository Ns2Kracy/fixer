use fixer_core::{HttpError, ProviderError};
use fixer_provider_musicbrainz::{MusicBrainzConfig, MusicBrainzError};
use std::time::Duration;

#[test]
fn defaults_identify_fixer_and_enforce_the_public_rate_policy() {
    let config = MusicBrainzConfig::default();

    assert_eq!(config.base_url().as_str(), "https://musicbrainz.org/ws/2/");
    assert_eq!(
        config.user_agent(),
        "ns2kracy/fixer/0.1.0 (https://github.com/ns2kracy/fixer)"
    );
    assert_eq!(config.minimum_request_interval(), Duration::from_secs(1));
}

#[test]
fn endpoint_identity_and_rate_overrides_are_validated() {
    let config = MusicBrainzConfig::default()
        .with_base_url("http://127.0.0.1:9000/ws/2/")
        .unwrap()
        .with_user_agent("fixer-integration/1 (maintainer@example.com)")
        .unwrap()
        .with_minimum_request_interval(Duration::from_millis(25))
        .unwrap();

    assert_eq!(config.base_url().as_str(), "http://127.0.0.1:9000/ws/2/");
    assert_eq!(config.minimum_request_interval(), Duration::from_millis(25));
    assert!(matches!(
        MusicBrainzConfig::default().with_base_url("file:///tmp/ws/2"),
        Err(MusicBrainzError::InvalidConfig(_))
    ));
    assert!(matches!(
        MusicBrainzConfig::default().with_user_agent("fixer"),
        Err(MusicBrainzError::InvalidConfig(_))
    ));
    assert!(matches!(
        MusicBrainzConfig::default().with_minimum_request_interval(Duration::ZERO),
        Err(MusicBrainzError::InvalidConfig(_))
    ));
}

#[test]
fn transport_failures_keep_stable_categories() {
    let cases = [
        (HttpError::Offline, "offline"),
        (HttpError::Timeout, "timeout"),
        (HttpError::Status { status: 404 }, "not_found"),
        (HttpError::Status { status: 429 }, "rate_limited"),
        (HttpError::Status { status: 503 }, "unexpected_status"),
    ];
    for (transport, expected) in cases {
        assert_eq!(MusicBrainzError::from_http(transport).code(), expected);
    }
    assert_eq!(
        ProviderError::from(MusicBrainzError::NotFound),
        ProviderError::NotFound
    );
}
