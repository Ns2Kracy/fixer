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
fn loopback_bind_addresses_are_accepted() {
    let ipv4 = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 8080);
    let ipv6 = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 8080);

    assert_eq!(ServerConfig::new(ipv4).unwrap().bind_addr(), ipv4);
    assert_eq!(ServerConfig::new(ipv6).unwrap().bind_addr(), ipv6);
}

#[test]
fn unauthenticated_non_loopback_binds_are_rejected_before_startup() {
    for bind_addr in [
        SocketAddr::from(([0, 0, 0, 0], 8080)),
        SocketAddr::from(([192, 168, 1, 20], 8080)),
        SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 8080),
    ] {
        let error = ServerConfig::new(bind_addr).unwrap_err();
        assert_eq!(
            error.to_string(),
            "non-loopback binding requires authentication"
        );
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

    let public = ServerConfig::parse("0.0.0.0:3000").unwrap_err();
    assert_eq!(
        public.to_string(),
        "non-loopback binding requires authentication"
    );
}

#[test]
fn authenticated_public_bind_configures_all_production_security_boundaries() {
    let root = tempfile::tempdir().unwrap();
    let bind = SocketAddr::from(([0, 0, 0, 0], 8443));
    let config = ServerConfig::authenticated(bind, "production password")
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
    let debug = format!("{config:?}");
    assert!(!debug.contains("production password"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn production_validation_requires_authentication_and_at_least_one_media_root() {
    let root = tempfile::tempdir().unwrap();
    let missing_auth = ServerConfig::default()
        .with_media_roots([root.path()])
        .unwrap();
    assert_eq!(
        missing_auth.validate_for_serve().unwrap_err().to_string(),
        "server authentication password is required"
    );

    let missing_roots =
        ServerConfig::authenticated(SocketAddr::from(([127, 0, 0, 1], 3000)), "password").unwrap();
    assert_eq!(
        missing_roots.validate_for_serve().unwrap_err().to_string(),
        "at least one allowed media root is required"
    );
}

#[test]
fn invalid_auth_origin_and_proxy_configuration_fail_before_listener_creation() {
    let loopback = SocketAddr::from(([127, 0, 0, 1], 3000));
    assert!(ServerConfig::authenticated(loopback, "").is_err());
    assert!(
        ServerConfig::authenticated(loopback, "password")
            .unwrap()
            .with_allowed_origins(["*"])
            .is_err()
    );
    assert!(
        ServerConfig::authenticated(loopback, "password")
            .unwrap()
            .with_trusted_proxy(["not-a-cidr"], "x-client-ip")
            .is_err()
    );
}
