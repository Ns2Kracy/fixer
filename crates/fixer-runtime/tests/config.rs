use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{Arc, Barrier},
    thread,
};

use fixer_runtime::{
    ConfigLoader, ConflictPolicy, FixerConfig, LoggingFormat, OutputPreset, PlacementPolicy,
};

fn env(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect()
}

#[test]
fn discovers_fixer_toml_and_deserializes_shared_and_server_sections() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("media")).unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        r#"
offline = true
local_root = "media"
preferred_locales = ["zh-Hans", "ja"]
timeout_seconds = 17
auto_accept_confidence = 0.8
review_confidence = 0.5
output_preset = "metadata"
placement = "copy"
conflict_policy = "prefer_first"
enabled_providers = ["local", "anilist"]

[providers.anilist]
base_url = "https://example.test/graphql"
access_token = "file-secret"

[server]
bind = "127.0.0.1:4312"
database = "state/fixer.sqlite3"
media_roots = ["media"]
web_root = "public"
allowed_origins = ["http://127.0.0.1:4312"]
worker_count = 3

[logging]
filter = "fixer_server=debug"
format = "json"
"#,
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();
    let config = loaded.config();

    assert_eq!(loaded.path(), root.path().join("fixer.toml"));
    assert!(config.offline);
    assert_eq!(
        config.local_root.as_deref(),
        Some(root.path().join("media").canonicalize().unwrap().as_path())
    );
    assert_eq!(config.preferred_locales, ["zh-Hans", "ja"]);
    assert_eq!(config.timeout_seconds, 17);
    assert_eq!(config.output_preset, OutputPreset::Metadata);
    assert_eq!(config.placement, PlacementPolicy::Copy);
    assert_eq!(config.conflict_policy, ConflictPolicy::PreferFirst);
    assert_eq!(config.enabled_provider_names(), ["local", "anilist"]);
    assert_eq!(config.server.bind.to_string(), "127.0.0.1:4312");
    assert_eq!(
        config.server.database,
        root.path().join("state/fixer.sqlite3")
    );
    assert_eq!(
        config.server.media_roots,
        [root.path().join("media").canonicalize().unwrap()]
    );
    assert_eq!(config.server.web_root, root.path().join("public"));
    assert_eq!(config.logging.format, LoggingFormat::Json);
}

#[test]
fn process_environment_overrides_current_directory_dotenv_and_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "timeout_seconds = 10\noffline = false\n",
    )
    .unwrap();
    fs::write(
        root.path().join(".env"),
        "FIXER_TIMEOUT_SECONDS=20\nFIXER_OFFLINE=false\n",
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[
            ("FIXER_TIMEOUT_SECONDS", "30"),
            ("FIXER_OFFLINE", "true"),
        ]))
        .load()
        .unwrap();

    assert_eq!(loaded.config().timeout_seconds, 30);
    assert!(loaded.config().offline);
}

#[test]
fn rust_log_overrides_nested_logging_environment_and_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "[logging]\nfilter = 'fixer_server=warn'\n",
    )
    .unwrap();
    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[
            ("FIXER_LOGGING__FILTER", "fixer_server=debug"),
            ("RUST_LOG", "fixer_server=trace,tower_http=debug"),
        ]))
        .load()
        .unwrap();

    assert_eq!(
        loaded.config().logging.filter,
        "fixer_server=trace,tower_http=debug"
    );
}

#[test]
fn current_directory_dotenv_is_loaded_when_process_environment_is_absent() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join(".env"),
        "FIXER_TIMEOUT_SECONDS=23\nFIXER_SERVER__BIND=127.0.0.1:4323\n",
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();

    assert_eq!(loaded.config().timeout_seconds, 23);
    assert_eq!(loaded.config().server.bind.to_string(), "127.0.0.1:4323");
}

#[test]
fn legacy_cli_json_is_normalized_below_environment_overrides() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("legacy.json"),
        r#"{
  "proxy": "http://file-proxy.example",
  "api_key": "file-tmdb-token",
  "tmdb_base_url": "https://file-tmdb.example/3",
  "anilist_enabled": true,
  "anilist_endpoint": "https://file-anilist.example/graphql",
  "anilist_token": "file-anilist-token",
  "secret_references": {
    "bangumi_access_token": "FILE_BANGUMI_TOKEN"
  }
}"#,
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_config_path("legacy.json")
        .with_environment(env(&[
            ("FIXER_PROXY", "http://env-proxy.example"),
            ("TMDB_API_TOKEN", "env-tmdb-token"),
            ("FIXER_BANGUMI_ACCESS_TOKEN", "env-bangumi-token"),
        ]))
        .load()
        .unwrap();
    let config = loaded.config();

    assert_eq!(config.proxy.as_deref(), Some("http://env-proxy.example"));
    assert_eq!(
        config.providers.tmdb.base_url,
        "https://file-tmdb.example/3"
    );
    assert_eq!(
        config.providers.tmdb.resolved_api_token(),
        Some("env-tmdb-token")
    );
    assert_eq!(
        config.providers.anilist.base_url,
        "https://file-anilist.example/graphql"
    );
    assert_eq!(
        config.providers.anilist.resolved_access_token(),
        Some("file-anilist-token")
    );
    assert!(config.provider_enabled("anilist"));
}

#[test]
fn malformed_dotenv_and_unknown_toml_fields_fail_closed() {
    let malformed = tempfile::tempdir().unwrap();
    fs::write(malformed.path().join(".env"), "NOT VALID\n").unwrap();
    let error = ConfigLoader::new(malformed.path())
        .with_environment(env(&[]))
        .load()
        .unwrap_err();
    assert!(error.to_string().contains(".env"));

    let unknown = tempfile::tempdir().unwrap();
    fs::write(unknown.path().join("fixer.toml"), "mystery = true\n").unwrap();
    let error = ConfigLoader::new(unknown.path())
        .with_environment(env(&[]))
        .load()
        .unwrap_err();
    assert!(error.to_string().contains("mystery"));
}

#[test]
fn explicit_json_config_remains_readable_but_is_not_auto_discovered() {
    let root = tempfile::tempdir().unwrap();
    let json = root.path().join("legacy.json");
    fs::write(&json, r#"{"offline":true,"timeout_seconds":41}"#).unwrap();
    fs::write(root.path().join("fixer.json"), r#"{"offline":true}"#).unwrap();

    let defaults = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();
    assert!(!defaults.config().offline);

    let explicit = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .with_config_path(&json)
        .load()
        .unwrap();
    assert!(explicit.config().offline);
    assert_eq!(explicit.config().timeout_seconds, 41);
}

#[test]
fn legacy_server_environment_names_override_nested_server_config() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("legacy-media")).unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "[server]\nbind = '127.0.0.1:3000'\ndatabase = 'file.sqlite3'\n",
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[
            ("FIXER_SERVER_BIND", "127.0.0.1:4555"),
            ("FIXER_SERVER_DATABASE", "legacy.sqlite3"),
            ("FIXER_SERVER_MEDIA_ROOTS", "legacy-media"),
            ("FIXER_WEB_ROOT", "legacy-web"),
        ]))
        .load()
        .unwrap();

    assert_eq!(loaded.config().server.bind.to_string(), "127.0.0.1:4555");
    assert_eq!(
        loaded.config().server.database,
        root.path().join("legacy.sqlite3")
    );
    assert_eq!(
        loaded.config().server.media_roots,
        [root.path().join("legacy-media").canonicalize().unwrap()]
    );
    assert_eq!(
        loaded.config().server.web_root,
        root.path().join("legacy-web")
    );
}

#[test]
fn debug_output_redacts_direct_secrets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "[providers.tmdb]\napi_token = 'tmdb-secret'\n[providers.anilist]\naccess_token = 'anilist-secret'\n",
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();
    let debug = format!("{:?}", loaded.config());

    assert!(!debug.contains("tmdb-secret"));
    assert!(!debug.contains("anilist-secret"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn config_handle_persists_atomically_and_updates_its_snapshot() {
    let root = tempfile::tempdir().unwrap();
    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();
    let handle = loaded.into_handle();
    let mut next = handle.snapshot();
    next.timeout_seconds = 52;

    handle.replace_and_persist(next).unwrap();

    assert_eq!(handle.snapshot().timeout_seconds, 52);
    assert!(handle.path().is_file());
    let reloaded = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap();
    assert_eq!(reloaded.config().timeout_seconds, 52);
}

#[test]
fn canonical_secret_environment_overrides_remain_runtime_only_when_persisted() {
    let root = tempfile::tempdir().unwrap();
    let environment = env(&[
        (
            "FIXER_PROVIDERS__TMDB__API_TOKEN",
            "environment-tmdb-secret",
        ),
        (
            "FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN",
            "environment-anilist-secret",
        ),
    ]);
    let handle = ConfigLoader::new(root.path())
        .with_environment(environment.clone())
        .load()
        .unwrap()
        .into_handle();

    assert_eq!(
        handle.snapshot().providers.tmdb.resolved_api_token(),
        Some("environment-tmdb-secret")
    );
    assert_eq!(
        handle.snapshot().providers.anilist.resolved_access_token(),
        Some("environment-anilist-secret")
    );

    let mut next = handle.snapshot();
    next.offline = true;
    handle.replace_and_persist(next).unwrap();

    let persisted = fs::read_to_string(handle.path()).unwrap();
    assert!(!persisted.contains("environment-tmdb-secret"));
    assert!(!persisted.contains("environment-anilist-secret"));
    let reloaded = ConfigLoader::new(root.path())
        .with_environment(environment)
        .load()
        .unwrap();
    assert_eq!(
        reloaded.config().providers.tmdb.resolved_api_token(),
        Some("environment-tmdb-secret")
    );
}

#[cfg(unix)]
#[test]
fn config_handle_writes_private_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap()
        .into_handle();
    let mut next = handle.snapshot();
    next.providers.tmdb.api_token = Some(fixer_runtime::SecretString::new("private-secret"));

    handle.replace_and_persist(next).unwrap();

    let mode = fs::metadata(handle.path()).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn config_handle_rejects_writing_toml_into_an_explicit_json_path() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("legacy.json");
    fs::write(&path, "{\"timeout_seconds\":41}\n").unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .with_config_path(&path)
        .load()
        .unwrap()
        .into_handle();
    let before = handle.snapshot();
    let original = fs::read(&path).unwrap();
    let mut next = before.clone();
    next.timeout_seconds = 52;

    let error = handle.replace_and_persist(next).unwrap_err();

    assert!(error.to_string().contains("TOML"));
    assert_eq!(handle.snapshot(), before);
    assert_eq!(fs::read(&path).unwrap(), original);
}

#[test]
fn malformed_legacy_boolean_fails_closed() {
    let root = tempfile::tempdir().unwrap();
    let error = ConfigLoader::new(root.path())
        .with_environment(env(&[("FIXER_ANILIST_ENABLED", "definitely-not")]))
        .load()
        .unwrap_err();

    assert!(error.to_string().contains("FIXER_ANILIST_ENABLED"));
}

#[test]
fn debug_output_redacts_environment_resolved_secrets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "[providers.tmdb]\napi_token_env = 'FIXER_TEST_TMDB_TOKEN'\n[providers.anilist]\naccess_token_env = 'FIXER_TEST_ANILIST_TOKEN'\n",
    )
    .unwrap();

    let loaded = ConfigLoader::new(root.path())
        .with_environment(env(&[
            ("FIXER_TEST_TMDB_TOKEN", "resolved-tmdb-secret"),
            ("FIXER_TEST_ANILIST_TOKEN", "resolved-anilist-secret"),
        ]))
        .load()
        .unwrap();
    let config = loaded.config();
    let debug = format!("{config:?}");

    assert_eq!(
        config.providers.tmdb.resolved_api_token(),
        Some("resolved-tmdb-secret")
    );
    assert_eq!(
        config.providers.anilist.resolved_access_token(),
        Some("resolved-anilist-secret")
    );
    assert!(!debug.contains("resolved-tmdb-secret"));
    assert!(!debug.contains("resolved-anilist-secret"));
    assert!(debug.contains("[REDACTED]"));
}

#[test]
fn validation_rejects_invalid_network_and_logging_policy() {
    let mut cases = Vec::new();

    let mut tmdb = FixerConfig::default();
    tmdb.providers.tmdb.base_url = "file:///tmp/api".to_owned();
    cases.push(("tmdb", tmdb));

    let mut bangumi = FixerConfig::default();
    bangumi.providers.bangumi.base_url = "file:///tmp/api".to_owned();
    cases.push(("bangumi", bangumi));

    let mut anilist = FixerConfig::default();
    anilist.providers.anilist.base_url = "file:///tmp/api".to_owned();
    cases.push(("anilist", anilist));

    let mut musicbrainz = FixerConfig::default();
    musicbrainz.providers.musicbrainz.base_url = "file:///tmp/api".to_owned();
    cases.push(("musicbrainz", musicbrainz));

    let mut openlibrary_api = FixerConfig::default();
    openlibrary_api.providers.openlibrary.base_url = "file:///tmp/api".to_owned();
    cases.push(("openlibrary", openlibrary_api));

    let mut openlibrary_cover = FixerConfig::default();
    openlibrary_cover.providers.openlibrary.cover_base_url = "file:///tmp/api".to_owned();
    cases.push(("openlibrary", openlibrary_cover));

    let proxy = FixerConfig {
        proxy: Some("file:///tmp/proxy".to_owned()),
        ..FixerConfig::default()
    };
    cases.push(("proxy", proxy));

    let mut origin = FixerConfig::default();
    origin.server.allowed_origins = vec!["https://example.com/private".to_owned()];
    cases.push(("origin", origin));

    let mut trusted_range = FixerConfig::default();
    trusted_range.server.trusted_proxy.ranges = vec!["not-a-cidr".to_owned()];
    cases.push(("trusted_proxy", trusted_range));

    let mut trusted_header = FixerConfig::default();
    trusted_header.server.trusted_proxy.header = "not a header".to_owned();
    cases.push(("trusted_proxy", trusted_header));

    let mut credential_header = FixerConfig::default();
    credential_header.server.trusted_proxy.ranges = vec!["10.0.0.0/8".to_owned()];
    credential_header.server.trusted_proxy.header = "authorization".to_owned();
    cases.push(("trusted_proxy", credential_header));

    let mut logging = FixerConfig::default();
    logging.logging.filter = "[".to_owned();
    cases.push(("logging", logging));

    for (expected, config) in cases {
        let error = config.validate().unwrap_err().to_string();
        assert!(
            error.to_ascii_lowercase().contains(expected),
            "expected `{expected}` in `{error}`"
        );
    }
}

#[test]
fn config_handle_rejects_nonexistent_replacement_media_roots() {
    let root = tempfile::tempdir().unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap()
        .into_handle();
    let mut next = handle.snapshot();
    next.server.media_roots = vec![Path::new("missing-relative-root").to_owned()];

    let error = handle.replace_and_persist(next).unwrap_err();

    assert!(error.to_string().contains("missing-relative-root"));
    assert!(handle.snapshot().server.media_roots.is_empty());
    assert!(!handle.path().exists());
}

#[test]
fn config_handle_re_resolves_changed_secret_references() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("fixer.toml"),
        "[providers.tmdb]\napi_token_env = 'OLD_TOKEN'\n",
    )
    .unwrap();
    let environment = env(&[("OLD_TOKEN", "old-secret"), ("NEW_TOKEN", "new-secret")]);
    let handle = ConfigLoader::new(root.path())
        .with_environment(environment.clone())
        .load()
        .unwrap()
        .into_handle();
    let mut next = handle.snapshot();
    next.providers.tmdb.api_token_env = Some("NEW_TOKEN".to_owned());

    handle.replace_and_persist(next).unwrap();

    let snapshot = handle.snapshot();
    assert_eq!(
        snapshot.providers.tmdb.resolved_api_token(),
        Some("new-secret")
    );
    let reloaded = ConfigLoader::new(root.path())
        .with_environment(environment)
        .load()
        .unwrap();
    assert_eq!(
        reloaded.config().providers.tmdb.resolved_api_token(),
        Some("new-secret")
    );
}

#[test]
fn concurrent_config_replacements_keep_disk_and_memory_consistent() {
    let root = tempfile::tempdir().unwrap();
    let handle = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap()
        .into_handle();
    let barrier = Arc::new(Barrier::new(16));
    let writes = (1..=16)
        .map(|timeout| {
            let handle = handle.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                let mut next = handle.snapshot();
                next.timeout_seconds = timeout;
                barrier.wait();
                handle.replace_and_persist(next)
            })
        })
        .collect::<Vec<_>>();

    for write in writes {
        write.join().unwrap().unwrap();
    }

    let memory_timeout = handle.snapshot().timeout_seconds;
    let disk_timeout = ConfigLoader::new(root.path())
        .with_environment(env(&[]))
        .load()
        .unwrap()
        .config()
        .timeout_seconds;
    assert_eq!(disk_timeout, memory_timeout);
}
