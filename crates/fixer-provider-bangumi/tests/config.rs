use fixer_core::{HttpError, ProviderError};
use fixer_provider_bangumi::{BangumiConfig, BangumiError};

#[test]
fn defaults_identify_fixer_against_the_production_api() {
    let config = BangumiConfig::default();

    assert_eq!(config.base_url().as_str(), "https://api.bgm.tv/");
    assert_eq!(
        config.user_agent(),
        "ns2kracy/fixer/0.1.0 (https://github.com/ns2kracy/fixer)"
    );
}

#[test]
fn endpoint_and_user_agent_overrides_are_validated() {
    let config = BangumiConfig::new()
        .with_base_url("http://127.0.0.1:9000")
        .unwrap()
        .with_user_agent("fixer-integration-test/1")
        .unwrap();

    assert_eq!(config.base_url().as_str(), "http://127.0.0.1:9000/");
    assert_eq!(config.user_agent(), "fixer-integration-test/1");
    assert!(matches!(
        BangumiConfig::new().with_base_url("file:///tmp/api"),
        Err(BangumiError::InvalidConfig(_))
    ));
    assert!(matches!(
        BangumiConfig::new().with_user_agent("  "),
        Err(BangumiError::InvalidConfig(_))
    ));
}

#[test]
fn transport_failures_keep_stable_categories() {
    let cases = [
        (HttpError::Offline, "offline"),
        (HttpError::Timeout, "timeout"),
        (HttpError::Status { status: 401 }, "unauthorized"),
        (HttpError::Status { status: 403 }, "forbidden"),
        (HttpError::Status { status: 404 }, "not_found"),
        (HttpError::Status { status: 429 }, "rate_limited"),
        (HttpError::Status { status: 503 }, "unexpected_status"),
    ];

    for (transport, expected) in cases {
        assert_eq!(BangumiError::from_http(transport).code(), expected);
    }

    assert_eq!(
        ProviderError::from(BangumiError::NotFound),
        ProviderError::NotFound
    );
}
