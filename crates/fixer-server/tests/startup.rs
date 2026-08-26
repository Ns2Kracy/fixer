use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use fixer_server::ServerConfig;

#[test]
fn server_defaults_to_ipv4_loopback_port_3000() {
    let config = ServerConfig::default();
    assert_eq!(
        config.bind_addr(),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 3000)
    );
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
