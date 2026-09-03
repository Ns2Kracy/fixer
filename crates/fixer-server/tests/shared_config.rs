use std::{
    fs,
    io::{BufRead, BufReader},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

use fixer_runtime::ConfigLoader;
use fixer_server::ServerConfig;

#[test]
fn server_config_consumes_the_shared_server_subsection() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("media")).unwrap();
    fs::create_dir(root.path().join("public")).unwrap();
    fs::create_dir(root.path().join("state")).unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        r#"
[server]
bind = "127.0.0.1:4321"
database = "state/fixer.sqlite3"
media_roots = ["media"]
web_root = "public"
allowed_origins = ["https://fixer.example"]
https_termination = true
worker_count = 3

[server.trusted_proxy]
ranges = ["10.0.0.0/8"]
header = "x-fixer-client-ip"
"#,
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(Default::default())
        .load()
        .unwrap();
    let server = ServerConfig::from_shared(&loaded.config().server).unwrap();

    assert_eq!(server.bind_addr().to_string(), "127.0.0.1:4321");
    assert_eq!(
        server.database_path(),
        root.path().join("state/fixer.sqlite3")
    );
    assert_eq!(
        server.media_roots(),
        &[root.path().join("media").canonicalize().unwrap()]
    );
    assert_eq!(server.worker_count().get(), 3);
    assert!(server.https_termination());
    assert_eq!(server.allowed_origins(), &["https://fixer.example"]);
    assert!(server.trusted_proxy_policy().is_enabled());
    assert_eq!(loaded.config().server.web_root, root.path().join("public"));
}

#[test]
fn server_binary_loads_fixer_toml_and_emits_json_startup_tracing() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("media")).unwrap();
    fs::create_dir(root.path().join("public")).unwrap();
    fs::create_dir(root.path().join("state")).unwrap();
    fs::write(root.path().join(".env"), "FIXER_SERVER_BIND=127.0.0.1:0\n").unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        r#"
[server]
database = "state/fixer.sqlite3"
media_roots = ["media"]
web_root = "public"

[logging]
filter = "fixer_server=info"
format = "json"
"#,
    )
    .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_fixer-server"))
        .current_dir(root.path())
        .env_clear()
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.contains("server listening") {
                let _ = sender.send(line);
                return;
            }
        }
    });

    let line = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("server did not emit startup tracing");
    let _ = child.kill();
    let _ = child.wait();

    let event: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(event["fields"]["message"], "server listening");
    assert!(event["fields"]["bind"].as_str().is_some());
}
