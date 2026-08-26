use fixer_core::{HttpError, ProviderError};
use fixer_provider_openlibrary::{OpenLibraryConfig, OpenLibraryError};

#[test]
fn defaults_use_official_endpoints_and_identify_fixer() {
    let config = OpenLibraryConfig::default();

    assert_eq!(config.api_base_url().as_str(), "https://openlibrary.org/");
    assert_eq!(
        config.cover_base_url().as_str(),
        "https://covers.openlibrary.org/b/"
    );
    assert_eq!(
        config.user_agent(),
        "ns2kracy/fixer/0.1.0 (https://github.com/ns2kracy/fixer)"
    );
}

#[test]
fn endpoint_and_identity_overrides_are_validated() {
    let config = OpenLibraryConfig::default()
        .with_api_base_url("http://127.0.0.1:9000/")
        .unwrap()
        .with_cover_base_url("http://127.0.0.1:9001/covers/")
        .unwrap()
        .with_user_agent("fixer-integration/1 (maintainer@example.com)")
        .unwrap();

    assert_eq!(config.api_base_url().as_str(), "http://127.0.0.1:9000/");
    assert_eq!(
        config.cover_base_url().as_str(),
        "http://127.0.0.1:9001/covers/"
    );
    assert!(matches!(
        OpenLibraryConfig::default().with_api_base_url("file:///tmp/openlibrary"),
        Err(OpenLibraryError::InvalidConfig(_))
    ));
    assert!(matches!(
        OpenLibraryConfig::default().with_user_agent("fixer"),
        Err(OpenLibraryError::InvalidConfig(_))
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
        assert_eq!(OpenLibraryError::from_http(transport).code(), expected);
    }
    assert_eq!(
        ProviderError::from(OpenLibraryError::NotFound),
        ProviderError::NotFound
    );
}
