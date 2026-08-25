use fixer_core::{Header, HttpClient, HttpError, HttpMethod, HttpRequest};
use fixer_http::{HttpConfig, ReqwestHttpClient};
use std::time::Duration;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn server(response: &'static [u8], delay: Duration) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await.unwrap();
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        stream.write_all(response).await.unwrap();
    });
    format!("http://{address}")
}

#[tokio::test]
async fn successful_get_returns_status_headers_and_body() {
    let url = server(
        b"HTTP/1.1 200 OK\r\ncontent-type: text/plain\r\ncontent-length: 2\r\n\r\nok",
        Duration::ZERO,
    )
    .await;
    let client = ReqwestHttpClient::new(HttpConfig::default()).unwrap();
    let response = client
        .execute(HttpRequest::new(HttpMethod::Get, url))
        .await
        .unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"ok");
    assert!(
        response
            .headers
            .iter()
            .any(|header| header.name() == "content-type")
    );
}

#[tokio::test]
async fn timeout_is_structured() {
    let url = server(
        b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n",
        Duration::from_millis(100),
    )
    .await;
    let client =
        ReqwestHttpClient::new(HttpConfig::default().with_timeout(Duration::from_millis(20)))
            .unwrap();
    let error = client
        .execute(HttpRequest::new(HttpMethod::Get, url))
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Timeout));
}

#[tokio::test]
async fn non_success_status_is_structured() {
    let url = server(
        b"HTTP/1.1 429 Too Many Requests\r\ncontent-length: 0\r\n\r\n",
        Duration::ZERO,
    )
    .await;
    let client = ReqwestHttpClient::new(HttpConfig::default()).unwrap();
    let error = client
        .execute(HttpRequest::new(HttpMethod::Get, url))
        .await
        .unwrap_err();
    assert!(matches!(error, HttpError::Status { status: 429 }));
}

#[test]
fn explicit_http_and_socks_proxies_parse_without_leaking_credentials() {
    for proxy in [
        "http://user:password@127.0.0.1:8080",
        "socks5://user:password@127.0.0.1:1080",
    ] {
        let config = HttpConfig::default().with_proxy(proxy).unwrap();
        let debug = format!("{config:?}");
        assert!(!debug.contains("password"));
        assert!(debug.contains("[REDACTED]"));
        ReqwestHttpClient::new(config).unwrap();
    }
    assert!(
        HttpConfig::default()
            .with_proxy("file:///tmp/proxy")
            .is_err()
    );
}

#[test]
fn request_debug_redacts_headers_and_url_credentials() {
    let request = HttpRequest::new(HttpMethod::Get, "https://example.invalid?api_key=secret")
        .with_header(Header::new("authorization", "Bearer secret").unwrap())
        .with_header(Header::new("cookie", "session=secret").unwrap());
    let debug = format!("{request:?}");
    assert!(!debug.contains("secret"));
    assert!(debug.contains("[REDACTED]"));
}
