use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    path::Path,
};

use fixer_server::ServerConfig;

#[test]
fn server_defaults_to_ipv4_loopback_port_3000() {
    let config = ServerConfig::default();
    assert_eq!(
        config.bind_addr(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000)
    );
    assert_eq!(config.database_path(), Path::new("fixer.sqlite3"));
}

#[test]
fn bind_addresses_are_accepted_for_database_backed_authentication() {
    for bind_addr in [
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8080),
        SocketAddr::from(([0, 0, 0, 0], 8080)),
        SocketAddr::from(([192, 168, 1, 20], 8080)),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8080),
    ] {
        assert_eq!(ServerConfig::new(bind_addr).unwrap().bind_addr(), bind_addr);
    }
}

#[test]
fn database_paths_are_explicit_and_non_empty() {
    let config = ServerConfig::default()
        .with_database_path("state/jobs.sqlite")
        .unwrap();
    assert_eq!(config.database_path(), Path::new("state/jobs.sqlite"));
    assert!(ServerConfig::default().with_database_path("").is_err());
}

#[test]
fn bind_strings_are_validated_before_listener_creation() {
    let invalid = ServerConfig::parse("not-an-address").unwrap_err();
    assert!(
        invalid
            .to_string()
            .starts_with("invalid server bind address")
    );

    let public = ServerConfig::parse("0.0.0.0:3000").unwrap();
    assert_eq!(public.bind_addr(), SocketAddr::from(([0, 0, 0, 0], 3000)));
}

#[test]
fn public_bind_configures_all_production_security_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let bind = SocketAddr::from(([0, 0, 0, 0], 8443));
    let config = ServerConfig::new(bind)
        .unwrap()
        .with_media_roots([root.path()])
        .unwrap()
        .with_https_termination(true)
        .with_allowed_origins(["https://fixer.example"])
        .unwrap()
        .with_trusted_proxy(["10.0.0.0/8"], "x-fixer-client-ip")
        .unwrap();

    assert_eq!(config.bind_addr(), bind);
    assert_eq!(config.media_roots(), &[root.path().canonicalize().unwrap()]);
    assert!(config.https_termination());
    assert_eq!(config.allowed_origins(), &["https://fixer.example"]);
    assert!(config.trusted_proxy_policy().is_enabled());
    assert!(!format!("{config:?}").contains("password"));
}

#[test]
fn production_validation_requires_only_an_allowed_media_root() {
    let root = tempfile::tempdir().unwrap();
    let ready = ServerConfig::default()
        .with_media_roots([root.path()])
        .unwrap();
    ready.validate_for_serve().unwrap();

    assert_eq!(
        ServerConfig::default()
            .validate_for_serve()
            .unwrap_err()
            .to_string(),
        "at least one allowed media root is required"
    );
}

#[test]
fn invalid_origin_and_proxy_configuration_fail_before_listener_creation() {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 3000));
    assert!(
        ServerConfig::new(loopback)
            .unwrap()
            .with_allowed_origins(["*"])
            .is_err()
    );
    assert!(
        ServerConfig::new(loopback)
            .unwrap()
            .with_trusted_proxy(["not-a-cidr"], "x-client-ip")
            .is_err()
    );
}
