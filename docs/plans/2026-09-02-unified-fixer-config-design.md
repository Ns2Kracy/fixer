# Unified Fixer Configuration and Observability Design

**Date:** 2026-09-02
**Status:** Approved

## Context

Fixer currently has two unrelated application configuration paths:

- `fixer-cli` reads `fixer.json`, manually merges selected environment variables and flags, validates provider settings, and constructs the configured `Fixer` SDK runtime.
- `fixer-server` reads only `FIXER_SERVER_*` process variables, starts local-only workers, and keeps a second in-memory settings model in `workspace.rs`.

The split causes observable drift. The CLI and Web UI expose similar provider fields but do not drive the same provider runtime. The server does not load the repository `.env`, does not initialize `tracing`, and creates an error-local atomic sequence that is labelled as a request ID even though it is not request-scoped.

## Decision

Use one canonical `fixer.toml` document, loaded through `config-rs`, for both CLI and server. Add one shared application assembly crate, `fixer-runtime`, that owns:

- the serializable configuration schema and defaults;
- configuration discovery and source precedence;
- `.env` loading;
- validation and secret redaction;
- provider construction and the shared `Fixer` SDK builder;
- a shared configuration handle used by the long-running server.

The CLI and server remain transport adapters. The CLI owns command parsing and terminal rendering. The server owns HTTP, authentication, persistence, job scheduling, and static Web delivery. Provider availability, endpoints, credentials, locales, proxy, timeout, offline mode, confidence policy, placement, and conflict policy come from the same shared configuration and runtime builder.

## Canonical configuration

The default file is `./fixer.toml`. An explicit path may still use another format supported by `config-rs`, but generated documentation and discovery use TOML.

```toml
offline = false
preferred_locales = ["zh-Hans", "zh-Hant", "ja", "en", "und"]
timeout_seconds = 30
auto_accept_confidence = 0.9
review_confidence = 0.6
output_preset = "full"
placement = "in_place"
conflict_policy = "review"
enabled_providers = ["local", "tmdb", "bangumi", "musicbrainz", "openlibrary"]

[providers.tmdb]
api_token_env = "TMDB_API_TOKEN"
base_url = "https://api.themoviedb.org/3"

[providers.bangumi]
base_url = "https://api.bgm.tv"

[providers.anilist]
access_token_env = "ANILIST_ACCESS_TOKEN"
base_url = "https://graphql.anilist.co"

[providers.musicbrainz]
base_url = "https://musicbrainz.org/ws/2"

[providers.openlibrary]
base_url = "https://openlibrary.org"
cover_base_url = "https://covers.openlibrary.org/b/"

[server]
bind = "127.0.0.1:3000"
database = "fixer.sqlite3"
media_roots = ["/media"]
web_root = "web/dist"
allowed_origins = ["http://127.0.0.1:3000"]
https_termination = false
worker_count = 2

[server.trusted_proxy]
ranges = []
header = "x-forwarded-for"

[logging]
filter = "fixer_server=info,tower_http=info"
format = "pretty"
```

Direct token fields remain accepted for compatibility, but environment-backed references are preferred. Debug and validation output must never print token values.

## Discovery and precedence

Configuration sources are applied in this order, from lowest to highest precedence:

1. typed built-in defaults;
2. selected configuration file;
3. environment source;
4. legacy environment compatibility overrides;
5. CLI flags for command-scoped overrides.

The selected file is:

1. CLI `--config FILE`, when available;
2. `FIXER_CONFIG`;
3. `./fixer.toml`, when present;
4. no file, using defaults and environment values.

Before `config-rs` reads the environment, `dotenvy::from_path(".env")` attempts to load exactly the current working directory's `.env`. A missing file is allowed. A malformed file is a startup/configuration error. Existing process variables are not overwritten by `.env` values.

Canonical nested environment variables use `__`, for example:

```dotenv
FIXER_SERVER__BIND=127.0.0.1:3000
FIXER_SERVER__DATABASE=fixer.sqlite3
FIXER_LOGGING__FILTER=fixer_server=debug,tower_http=trace
FIXER_PROVIDERS__TMDB__API_TOKEN_ENV=TMDB_API_TOKEN
```

Existing flat CLI variables such as `FIXER_OFFLINE` and `FIXER_TIMEOUT_SECONDS` continue to work naturally because those fields remain top-level. Existing `FIXER_SERVER_BIND`, `FIXER_SERVER_DATABASE`, `FIXER_SERVER_MEDIA_ROOTS`, `FIXER_SERVER_ALLOWED_ORIGINS`, `FIXER_WEB_ROOT`, and trusted-proxy variables are mapped as compatibility overrides. Obsolete `FIXER_SERVER_PASSWORD` is ignored rather than becoming an unknown configuration field.

`fixer.json` is no longer auto-discovered. An explicitly selected JSON file may still be read during migration.

## Shared runtime and capability parity

`fixer-runtime` exposes a single provider builder. Both adapters call the same implementation with scan-derived `LocalProvider` data:

```rust
pub fn build_fixer(
    config: &FixerConfig,
    local_provider: LocalProvider,
) -> Result<Fixer, RuntimeConfigError>;
```

This function owns deterministic provider registration order, endpoint configuration, credentials, preferred languages, timeout, proxy, and offline behavior.

Server jobs must no longer use `WorkerFlow::Local` in production. `SdkJobFlow` stores the shared configuration handle. For each claimed job it:

1. scans local metadata and derives the local provider, title, ISBN, and output root;
2. snapshots the current shared configuration;
3. builds the same configured `Fixer` runtime used by CLI;
4. performs search, resolution, planning, and execution through the existing job state machine.

Capability parity means CLI and server use the same provider/runtime policy. Transport-only features remain adapter-specific: terminal rendering and flags belong to CLI; bind address, database, CORS, authentication, and job persistence belong to server.

## Web settings and persistence

The server owns a `ConfigHandle` containing the selected path and an `Arc<RwLock<FixerConfig>>`.

- `GET /api/v1/settings` reads a redacted snapshot from the handle.
- `PUT /api/v1/settings` validates a complete replacement of user-editable shared settings.
- A successful update writes `fixer.toml` atomically using a temporary sibling file and rename, then replaces the in-memory snapshot.
- Failed validation or persistence leaves both disk and memory unchanged.
- Workers snapshot the handle per job, so new jobs use updated settings without restarting; active jobs retain the snapshot with which they began.
- Server bootstrap/security fields are not writable through the settings API.

The existing Web DTO remains stable where practical, but its internal duplicate enums, defaults, provider constants, and validation move to shared configuration types.

## Tracing and request IDs

The server initializes `tracing-subscriber` before opening SQLite or binding the listener. `RUST_LOG` overrides `logging.filter`; otherwise the configured filter is used.

Every API request passes through this Tower order:

1. `SetRequestIdLayer<MakeRequestUuid>` ensures `x-request-id` exists and stores a `RequestId` extension;
2. `PropagateRequestIdLayer` copies the same ID to the response;
3. `TraceLayer` creates an HTTP span containing request ID, method, URI, and version and records response status and latency;
4. authentication, CORS, handlers, and error conversion run inside that span.

`ApiError` stops generating an atomic sequence. Error metadata is attached to the response, and a response middleware serializes the final error envelope using the request-scoped ID. The response header, JSON `error.request_id`, and tracing span therefore contain the same value. Successful API responses also carry `x-request-id`.

## Error handling and security

- Unknown TOML fields fail closed through `serde(deny_unknown_fields)`.
- Invalid provider IDs, endpoints, confidence ranges, roots, origins, proxy policy, and log filters fail before listener creation.
- Secrets are redacted in `Debug`, validation summaries, logs, and API snapshots.
- Request tracing does not include request or response headers or bodies by default.
- `.env` and `fixer.toml` are ignored by Git; a committed `fixer.toml.example` contains placeholders only.
- Web configuration writes are authenticated and CSRF-protected by the existing middleware.

## Migration

- Change default discovery from `fixer.json` to `fixer.toml`.
- Keep explicit JSON file loading through `--config`/`FIXER_CONFIG` for migration.
- Preserve existing flat CLI environment variables.
- Preserve existing server environment variables through explicit compatibility mapping.
- Update Compose, README, configuration, server, troubleshooting, and security documentation to use the canonical TOML model.

## Sources

- config-rs 0.15.19: <https://docs.rs/config/0.15.19/config/>
- config-rs `ConfigBuilder`: <https://docs.rs/config/0.15.19/config/builder/struct.ConfigBuilder.html>
- config-rs `Environment`: <https://docs.rs/config/0.15.19/config/struct.Environment.html>
- dotenvy 0.15.7: <https://docs.rs/dotenvy/0.15.7/dotenvy/>
- tower-http request IDs 0.6.11: <https://docs.rs/tower-http/0.6.11/tower_http/request_id/index.html>
- tower-http tracing 0.6.11: <https://docs.rs/tower-http/0.6.11/tower_http/trace/index.html>
- tracing-subscriber `EnvFilter` 0.3.22: <https://docs.rs/tracing-subscriber/0.3.22/tracing_subscriber/filter/struct.EnvFilter.html>
