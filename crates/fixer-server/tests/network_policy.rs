use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use axum::http::HeaderMap;
use fixer_server::TrustedProxyPolicy;

#[test]
fn forwarded_identity_is_ignored_when_proxy_trust_is_disabled() {
    let peer = SocketAddr::from(([127, 0, 0, 1], 4321));
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.7".parse().unwrap());

    assert_eq!(
        TrustedProxyPolicy::disabled().client_ip(peer, &headers),
        peer.ip()
    );
}

#[test]
fn only_the_exact_configured_header_from_a_trusted_cidr_can_override_identity() {
    let policy = TrustedProxyPolicy::new(["10.20.0.0/16"], "x-fixer-client-ip").unwrap();
    let trusted = SocketAddr::from(([10, 20, 1, 9], 4321));
    let untrusted = SocketAddr::from(([10, 21, 1, 9], 4321));
    let mut headers = HeaderMap::new();
    headers.insert("x-fixer-client-ip", "203.0.113.7".parse().unwrap());
    headers.insert("x-forwarded-for", "198.51.100.2".parse().unwrap());

    assert_eq!(
        policy.client_ip(trusted, &headers),
        IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))
    );
    assert_eq!(policy.client_ip(untrusted, &headers), untrusted.ip());

    headers.insert(
        "x-fixer-client-ip",
        "203.0.113.7, 198.51.100.2".parse().unwrap(),
    );
    assert_eq!(policy.client_ip(trusted, &headers), trusted.ip());
}

#[test]
fn proxy_configuration_rejects_invalid_ranges_and_unsafe_header_names() {
    assert!(TrustedProxyPolicy::new(["not-a-cidr"], "x-client-ip").is_err());
    assert!(TrustedProxyPolicy::new(["10.0.0.0/8"], "authorization").is_err());
    assert!(TrustedProxyPolicy::new(["10.0.0.0/8"], "cookie").is_err());
    assert!(TrustedProxyPolicy::new(["10.0.0.0/8"], "x-forwarded-for").is_ok());
}
