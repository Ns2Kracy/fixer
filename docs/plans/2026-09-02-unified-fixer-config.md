# Unified Fixer Configuration and Observability Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Make CLI and Web Server load one canonical `fixer.toml`, construct the same configured Fixer runtime, persist Web settings to that file, and emit correlated tracing/request IDs.

**Architecture:** Add one `fixer-runtime` crate containing the shared config-rs schema, loader, mutable server handle, and provider builder. Keep CLI and server as thin adapters; server workers snapshot the shared handle per job and build the same provider runtime as CLI from scan-derived local metadata. Add Tower request-ID and tracing layers around every API request.

**Tech Stack:** Rust 2024, config-rs 0.15.19, dotenvy 0.15.7, serde, TOML, Axum 0.8.9, tower-http 0.6.11, tracing 0.1.44, tracing-subscriber 0.3.22, Tokio, existing Fixer SDK/providers/writers.

---

### Task 1: Add the shared `fixer-runtime` crate and configuration schema

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-runtime/Cargo.toml`
- Create: `crates/fixer-runtime/src/lib.rs`
- Create: `crates/fixer-runtime/src/config.rs`
- Create: `crates/fixer-runtime/tests/config.rs`
- Create: `fixer.toml.example`
- Modify: `.gitignore`
- Modify: `Cargo.lock`

**Step 1: Write failing loader tests**

Add integration tests covering:

```rust
#[test]
fn discovers_fixer_toml_and_deserializes_shared_and_server_sections() { /* ... */ }

#[test]
fn process_environment_overrides_current_directory_dotenv_and_file() { /* ... */ }

#[test]
fn current_directory_dotenv_is_loaded_when_process_environment_is_absent() { /* ... */ }

#[test]
fn malformed_dotenv_and_unknown_toml_fields_fail_closed() { /* ... */ }

#[test]
fn explicit_json_config_remains_readable_but_is_not_auto_discovered() { /* ... */ }

#[test]
fn legacy_server_environment_names_override_nested_server_config() { /* ... */ }

#[test]
fn debug_output_redacts_direct_and_resolved_secrets() { /* ... */ }
```

Use a process-wide mutex for tests that change current directory or environment. Restore every variable and working directory in a guard.

**Step 2: Run the tests and confirm RED**

Run:

```bash
cargo test -p fixer-runtime --test config
```

Expected: FAIL because the crate and public symbols do not exist.

**Step 3: Add workspace dependencies and crate manifest**

Add workspace dependencies:

```toml
config = "0.15.19"
dotenvy = "0.15.7"
toml = "1.1.4"
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.22", features = ["env-filter", "fmt", "json"] }
```

Create `fixer-runtime` with dependencies on the existing core, SDK, HTTP, provider, and local-writer crates plus config/serde/TOML dependencies.

**Step 4: Implement typed configuration**

Export these stable boundaries from `crates/fixer-runtime/src/lib.rs`:

```rust
pub use config::{
    ConfigHandle, ConfigLoadError, ConfigLoader, ConflictPolicy, FixerConfig, LoadedConfig,
    LoggingConfig, LoggingFormat, OutputPreset, PlacementPolicy, ProviderEndpoints,
    ServerConfig,
};

pub fn build_fixer(
    config: &FixerConfig,
    local_provider: fixer_provider_local::LocalProvider,
) -> Result<fixer_sdk::Fixer, RuntimeConfigError>;
```

Use `#[serde(default, deny_unknown_fields)]` and explicit nested provider/server/logging structs. Keep common fields top-level so existing `FIXER_OFFLINE`, `FIXER_PROXY`, and similar environment variables map naturally.

**Step 5: Implement discovery and source precedence**

`ConfigLoader` must:

1. call `dotenvy::from_path(".env")` and ignore only `Error::Io(NotFound)`;
2. select explicit path, `FIXER_CONFIG`, or `./fixer.toml`;
3. add `File::from(path)` when selected;
4. add `Environment::with_prefix("FIXER").prefix_separator("_").separator("__").try_parsing(true)` using a filtered environment map that excludes `FIXER_CONFIG`, obsolete password configuration, and legacy flat server keys;
5. apply legacy flat server variables as final overrides;
6. deserialize and validate `FixerConfig`.

**Step 6: Implement atomic persistence and redaction**

`ConfigHandle` stores the selected path and `Arc<RwLock<FixerConfig>>`. Implement:

```rust
pub fn snapshot(&self) -> FixerConfig;
pub fn replace_and_persist(&self, next: FixerConfig) -> Result<(), ConfigWriteError>;
```

Serialize to TOML, write a temporary sibling, sync, and rename. Replace memory only after persistence succeeds. Redact tokens in every `Debug` implementation.

**Step 7: Add example and ignore private config**

Add `.env`, `.env.*`, and `fixer.toml` to `.gitignore` while keeping `fixer.toml.example` committed. The example contains placeholders, no real credentials.

**Step 8: Run tests and confirm GREEN**

Run:

```bash
cargo test -p fixer-runtime --test config
cargo test -p fixer-runtime
```

Expected: PASS.

**Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore fixer.toml.example crates/fixer-runtime
git commit -m "feat(config): add shared fixer.toml runtime configuration"
```

### Task 2: Move provider construction and CLI configuration to `fixer-runtime`

**Files:**

- Modify: `crates/fixer-runtime/src/lib.rs`
- Modify: `crates/fixer-runtime/src/config.rs`
- Modify: `crates/fixer-cli/Cargo.toml`
- Modify: `crates/fixer-cli/src/config.rs`
- Modify: `crates/fixer-cli/src/commands/mod.rs`
- Modify: `crates/fixer-cli/src/main.rs`
- Modify: `crates/fixer-cli/tests/cli.rs`
- Modify: `crates/fixer-cli/tests/*_config.rs`
- Modify: `docs/configuration.md`

**Step 1: Write failing CLI parity tests**

Update tests to use `fixer.toml` and add checks that:

- auto-discovery ignores `fixer.json` and reads `fixer.toml`;
- explicit JSON still works through `--config`;
- TOML, environment, and flags keep flag > environment > file > default precedence;
- validation output remains secret-safe;
- every configured provider can be constructed through the shared builder.

**Step 2: Run targeted CLI tests and confirm RED**

Run:

```bash
cargo test -p fixer-cli --test cli --test anilist_config --test book_config --test anime_providers
```

Expected: FAIL on old discovery and old private config implementation.

**Step 3: Implement the shared provider builder**

Move deterministic registration from `crates/fixer-cli/src/commands/mod.rs::build_fixer` into `fixer_runtime::build_fixer`. Preserve registration order:

1. local;
2. TMDB;
3. Bangumi;
4. MusicBrainz;
5. Open Library;
6. AniList.

Resolve direct environment credentials before configured references and direct file values using the documented compatibility order. Reuse provider crates' own URL validation.

**Step 4: Reduce CLI config to an adapter**

Replace the 700-line manual loader with a small wrapper that:

- calls `ConfigLoader` with `cli.config`;
- applies non-persistent command overrides for offline, proxy, local root, and placement;
- exposes the existing validation summary without duplicating provider construction or validation.

Replace local `OutputPreset`, `PlacementPolicy`, and `ConflictPolicy` with shared types or re-exports.

**Step 5: Replace CLI builder calls**

Change `commands::build_fixer` to delegate directly:

```rust
fixer_runtime::build_fixer(config.shared(), provider).map_err(AppError::new)
```

Do not keep a second provider-registration implementation.

**Step 6: Run targeted and full CLI tests**

Run:

```bash
cargo test -p fixer-cli --test cli --test anilist_config --test book_config --test anime_providers
cargo test -p fixer-cli
```

Expected: PASS.

**Step 7: Commit**

```bash
git add crates/fixer-runtime crates/fixer-cli docs/configuration.md Cargo.lock
git commit -m "refactor(cli): use shared fixer runtime configuration"
```

### Task 3: Make server startup and workers use the shared configuration

**Files:**

- Modify: `crates/fixer-server/Cargo.toml`
- Modify: `crates/fixer-server/src/main.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Modify: `crates/fixer-server/src/jobs/worker.rs`
- Modify: `crates/fixer-server/src/jobs/mod.rs`
- Modify: `crates/fixer-server/tests/startup.rs`
- Modify: `crates/fixer-server/tests/jobs.rs`
- Modify: `crates/fixer-server/tests/e2e_jobs.rs`

**Step 1: Write failing server parity tests**

Add tests proving:

- `fixer-server` obtains bind/database/media roots/Web root from the shared TOML loader;
- a configured server job scans a local item and receives candidates from the same enabled network providers as CLI;
- offline mode and provider allowlists behave identically in both adapters;
- a new job snapshots updated configuration while an already-started job keeps its original snapshot.

**Step 2: Run targeted tests and confirm RED**

Run:

```bash
cargo test -p fixer-server --test startup --test jobs --test e2e_jobs
```

Expected: FAIL because production still calls `start_local_workers` and `ServerConfig::from_env`.

**Step 3: Adapt server startup**

In `main.rs`:

```rust
let loaded = ConfigLoader::default().load()?;
fixer_server::init_tracing(&loaded.config.logging)?;
fixer_server::serve(loaded.into_handle()).await?;
```

Re-export or adapt `ServerConfig` so existing library construction tests remain focused on server validation rather than environment parsing. Remove the manual `optional_env`, boolean, and CSV loaders after compatibility behavior is covered by `fixer-runtime`.

**Step 4: Make `SdkJobFlow` configuration-driven**

Change `SdkJobFlow` to hold `ConfigHandle`, not a prebuilt `Fixer`. Refactor the local scan function to always return scan-derived `LocalProvider`, title, ISBN, and output root. For configured jobs, snapshot the handle and call `fixer_runtime::build_fixer`; keep an explicit local-only flow only for isolated tests that require it.

Start production workers with:

```rust
runtime.start_workers(worker_count, SdkJobFlow::new(config.clone()))
```

**Step 5: Run targeted server tests**

Run:

```bash
cargo test -p fixer-server --test startup --test jobs --test e2e_jobs
```

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-server crates/fixer-runtime Cargo.lock
git commit -m "feat(server): run jobs with shared configured providers"
```

### Task 4: Bind Web settings to the shared `fixer.toml`

**Files:**

- Modify: `crates/fixer-server/src/workspace.rs`
- Modify: `crates/fixer-server/src/api/v1/workspace.rs`
- Modify: `crates/fixer-server/src/app.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Modify: `crates/fixer-server/tests/workspace.rs`
- Modify: `crates/fixer-server/tests/api.rs`
- Modify: `web/src/lib/api.ts` only if the stable DTO requires an additive field
- Modify: related Web tests only when required by the DTO

**Step 1: Write failing settings persistence tests**

Cover:

- startup settings reflect `fixer.toml` rather than hard-coded defaults;
- GET redacts direct and environment-resolved secrets;
- PUT persists valid editable settings atomically;
- failed persistence leaves memory and disk unchanged;
- server bootstrap/security fields cannot be changed through PUT;
- the next worker job sees the updated provider policy.

**Step 2: Run tests and confirm RED**

Run:

```bash
cargo test -p fixer-server --test workspace --test api
```

Expected: FAIL because `WorkspaceState` owns an unrelated `RwLock<WorkspaceSettings>`.

**Step 3: Remove duplicate configuration state**

Give `WorkspaceState` a cloned `ConfigHandle`. Build `WorkspaceSettingsSnapshot` from shared types. Convert `WorkspaceSettingsInput` into a validated candidate `FixerConfig`, preserving server-only fields and retained secrets when clear flags are false.

Perform the blocking TOML write through `tokio::task::spawn_blocking`, then return the redacted persisted snapshot.

**Step 4: Share the same handle with workers and routes**

Create one handle in `serve`, clone it into `SdkJobFlow` and `WorkspaceState`, and keep authentication/database state separate.

**Step 5: Run server and Web contract tests**

Run:

```bash
cargo test -p fixer-server --test workspace --test api
pnpm --dir web test -- --run
```

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-server web
git commit -m "feat(server): persist Web settings to fixer.toml"
```

### Task 5: Add tracing and coherent request IDs

**Files:**

- Modify: `crates/fixer-server/Cargo.toml`
- Create: `crates/fixer-server/src/observability.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Modify: `crates/fixer-server/src/app.rs`
- Modify: `crates/fixer-server/src/api/error.rs`
- Modify: `crates/fixer-server/tests/api.rs`
- Create or modify: `crates/fixer-server/tests/observability.rs`
- Modify: `Cargo.lock`

**Step 1: Write failing request-ID tests**

Add router-level tests proving:

```rust
#[tokio::test]
async fn success_and_error_responses_always_have_uuid_request_ids() { /* ... */ }

#[tokio::test]
async fn supplied_request_id_is_propagated_consistently() { /* ... */ }

#[tokio::test]
async fn error_header_body_and_trace_span_use_the_same_request_id() { /* ... */ }

#[tokio::test]
async fn independent_requests_do_not_reuse_atomic_sequence_ids() { /* ... */ }
```

Capture tracing events with a test subscriber/writer; assert fields, not formatted timestamps.

**Step 2: Run tests and confirm RED**

Run:

```bash
cargo test -p fixer-server --test api --test observability
```

Expected: FAIL because successful responses lack IDs, errors use `AtomicU64`, and no subscriber/layer exists.

**Step 3: Initialize tracing**

Add `init_tracing(&LoggingConfig)` using `EnvFilter::try_from_default_env()` with configured fallback. Support pretty and JSON formats without logging headers or bodies.

**Step 4: Add Tower observability layers**

Create a `ServiceBuilder` in documented order:

```rust
ServiceBuilder::new()
    .layer(SetRequestIdLayer::new(X_REQUEST_ID.clone(), MakeRequestUuid))
    .layer(PropagateRequestIdLayer::new(X_REQUEST_ID.clone()))
    .layer(request_trace_layer())
    .layer(axum::middleware::from_fn(finalize_api_error));
```

The trace span records request ID, method, URI, version, response status, latency, and failures.

**Step 5: Make error IDs request-scoped**

Remove `NEXT_REQUEST_ID`. `ApiError::into_response` attaches serializable error metadata to response extensions. `finalize_api_error` obtains `tower_http::request_id::RequestId` from request extensions, replaces the JSON body, and preserves status and headers. Ensure method-not-allowed `Allow`, cookies, and other response headers survive.

**Step 6: Run observability and full server tests**

Run:

```bash
cargo test -p fixer-server --test api --test observability
cargo test -p fixer-server
```

Expected: PASS.

**Step 7: Commit**

```bash
git add crates/fixer-server Cargo.lock
git commit -m "feat(server): add tracing and request scoped IDs"
```

### Task 6: Migrate deployment and user documentation

**Files:**

- Modify: `README.md`
- Modify: `docs/configuration.md`
- Modify: `docs/server.md`
- Modify: `docs/development.md`
- Modify: `docs/troubleshooting.md`
- Modify: `docs/security.md`
- Modify: `.env.docker.example`
- Modify: `compose.yaml`
- Modify: `compose.build.yaml` if necessary
- Modify: `Dockerfile` if the default config path/layout requires it
- Modify: `scripts/e2e-local.sh`

**Step 1: Add documentation contract checks**

Create a bounded shell probe or extend existing documentation tests to fail while references to auto-discovered `fixer.json` or server-only configuration instructions remain.

**Step 2: Update examples and migration guidance**

Document:

- `fixer.toml` discovery and an example file;
- `.env` current-directory behavior;
- canonical `FIXER_*`/nested `__` variables;
- legacy server-variable compatibility;
- shared CLI/Web provider behavior;
- authenticated Web settings persistence;
- request-ID response header and error body correlation;
- `RUST_LOG` and `[logging]` behavior;
- explicit JSON migration support.

Update Compose to mount or generate one private `fixer.toml` and retain environment injection for secrets and deployment overrides.

**Step 3: Run documentation and deployment probes**

Run:

```bash
cargo run -p fixer-cli -- --help
cargo run -p fixer-cli -- config validate
docker compose --env-file .env.docker.example config --quiet
scripts/e2e-local.sh
```

Use a temporary config and media directory; never print or commit real secrets.

**Step 4: Commit**

```bash
git add README.md docs .env.docker.example compose.yaml compose.build.yaml Dockerfile scripts/e2e-local.sh fixer.toml.example
git commit -m "docs: migrate CLI and server to fixer.toml"
```

### Task 7: Final verification and review

**Files:**

- Review all changed files
- Modify only defects found by verification/review

**Step 1: Run proactive diagnostics**

Run LSP diagnostics on all touched Rust files, then fix every error and inspect warnings introduced by the change.

**Step 2: Run formatting and linting**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: PASS.

**Step 3: Run complete tests**

```bash
cargo test --workspace --all-targets
pnpm --dir web test -- --run
pnpm --dir web build
```

Expected: PASS with zero skipped/failed tests.

**Step 4: Run direct behavioral probes**

Start the server from a temporary directory containing `.env` and `fixer.toml`; verify:

- startup logs include the effective bind address but no secrets;
- a successful health request returns `x-request-id`;
- an API error returns the same ID in header, JSON body, and trace event;
- CLI and Web provider lists match for the same file;
- a Web settings update changes `fixer.toml` and affects a subsequent job.

**Step 5: Review migration and security boundaries**

Confirm mechanically:

- no auto-discovery references to `fixer.json` remain;
- no production call to `start_local_workers` remains;
- CLI and server both reference `fixer_runtime::build_fixer`;
- no atomic request-ID generator remains;
- no secret values appear in Debug, logs, snapshots, examples, or staged diff;
- user-owned pre-existing changes remain uncommitted and unmodified unless required.

**Step 6: Request code review and fix findings**

Use the code-review skill across correctness, security, API compatibility, operational behavior, and test quality. Re-run only checks affected by review fixes.

**Step 7: Commit verification fixes**

```bash
git add <only files changed by review fixes>
git commit -m "fix: address unified configuration review"
```
