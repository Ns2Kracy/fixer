# Development

Fixer is a Rust 2024 workspace with a Solid/Vite Web application. Default tests use committed fixtures or local mock servers and must not require public network access.

## Prerequisites

- Rust 1.85 or newer, matching `workspace.package.rust-version`
- Node.js 24.15 or newer
- pnpm 11.22.0, pinned by `web/package.json`
- `curl` for the local production browser harness
- Chromium installed by Playwright, or a compatible local Chrome channel

Install locked Web dependencies:

```bash
pnpm --dir web install --frozen-lockfile
```

Cargo uses `Cargo.lock`; use `--locked` in reproducible build scripts.

## Workspace map

| Path | Responsibility |
| --- | --- |
| `crates/fixer-core` | runtime-neutral domain models, provider/HTTP/writer contracts, matching, merge, and output plans |
| `crates/fixer-sdk` | Tokio orchestration, typed query flows, offline fixture provider, and output execution |
| `crates/fixer-http` | rustls-backed Reqwest transport |
| `crates/fixer-provider-*` | compile-time local and network providers |
| `crates/fixer-writer-local` | planning-only JSON/NFO/hierarchy/template/manifest writers |
| `crates/fixer-cli` | Clap configuration and cross-media workflows |
| `crates/fixer-server` | Axum API, authentication, SQLite jobs, workers, and static Web serving |
| `web` | Solid workspace, Vitest unit tests, and Playwright browser acceptance |
| `tests/fixtures/library` | shared cross-interface offline library fixtures |

See [SDK usage](sdk.md), [providers](providers.md), and [output execution](output.md) before changing a public contract.

## Fast local loop

Format and test the crate you changed:

```bash
cargo fmt --all -- --check
cargo check -p fixer-sdk --all-targets
cargo test -p fixer-sdk
```

Replace `fixer-sdk` with the target package. Run one integration target with:

```bash
cargo test -p fixer-cli --test e2e_library
cargo test -p fixer-server --test e2e_jobs
```

Web loop:

```bash
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

Vitest is restricted to `web/src/**/*.test.{ts,tsx}`. Playwright owns `web/e2e/**/*.spec.ts`.

## Complete gates

Before merging a cross-cutting change, run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --workspace --doc --locked
pnpm --dir web install --frozen-lockfile --offline
pnpm --dir web test
pnpm --dir web build
```

`--offline` on the pnpm install verifies the local dependency store only; omit it on a fresh machine. Rustdoc tests include the SDK builder and typed movie examples. Compile-fail contracts run through the normal Rust suite.

Run `git diff --check` before staging documentation or code.

## CLI development

Use Cargo to run the binary without installing it:

```bash
cargo run -p fixer-cli -- --help
cargo run -p fixer-cli -- providers list
cargo run -p fixer-cli -- config validate
cargo run -p fixer-cli -- scan tests/fixtures/library --kind movie --json
```

Global flags follow `--` because they belong to `fixer`, not Cargo. See [CLI workflows](cli.md) for every subcommand, exit code, and schema-versioned JSON DTO.

Run the registered SDK example with:

```bash
cargo run -p fixer-sdk --example sdk_movie
```

## Server and Web development

Run Axum and Vite in separate terminals:

```bash
FIXER_SERVER_PASSWORD='local-development-password' \
FIXER_SERVER_MEDIA_ROOTS='/absolute/path/to/media' \
FIXER_SERVER_ALLOWED_ORIGINS='http://127.0.0.1:5173' \
cargo run -p fixer-server
```

```bash
pnpm --dir web dev
```

Vite listens on `127.0.0.1:5173` and proxies `/api` to `127.0.0.1:3000`. Keep browser requests on the Vite origin. Use a disposable `FIXER_SERVER_DATABASE` for tests that should not touch the default `./fixer.sqlite3`.

The production layout requires `pnpm --dir web build`; Axum then serves `web/dist`. See [server operations](server.md).

## Browser acceptance

Install the pinned Playwright browser and run the self-contained production flow:

```bash
pnpm --dir web test:e2e:install
scripts/e2e-local.sh
```

The harness:

- copies the committed movie fixture to a canonical temporary media root;
- builds Web assets and the Cargo-reported `fixer-server` executable;
- creates isolated SQLite/auth/origin/media settings;
- signs in, creates and reviews a job, approves one bounded write, and verifies output/source bytes;
- terminates browser/server processes and removes temporary state.

It can take longer than 60 seconds on a cold Cargo or browser cache. Test runners need an explicit timeout that covers builds and browser provisioning.

Use an installed system Chrome only when Playwright Chromium cannot run:

```bash
FIXER_E2E_BROWSER_CHANNEL=chrome scripts/e2e-local.sh
```

`FIXER_E2E_PORT` pins the port for debugging. Without it, the harness chooses a new ephemeral port and retries early bind failures.

## Fixture policy

Deterministic tests must use one of these sources:

- small sanitized provider payloads under `crates/fixer-provider-*/tests/fixtures`;
- local Wiremock servers that serve those payloads;
- shared media trees under `tests/fixtures/library`;
- `FixtureProvider` for typed in-memory SDK tests;
- temporary directories/databases for output and server tests.

Fixtures must contain no real credentials, cookies, private library paths, or unnecessary provider payload fields. Keep them small enough to review. Add malformed and boundary fixtures when parser behavior changes.

Do not make default tests depend on DNS, public API uptime, account quotas, region routing, or current provider data. Endpoint overrides exist so provider tests can use local servers.

## Online-test policy

Live network tests are opt-in and ignored by default. The current TMDB smoke test requires an explicit token and network access:

Load `TMDB_API_TOKEN` from a secret manager into the test process without typing the token into shell history, then run:

```bash
cargo test -p fixer-provider-tmdb --test movie live_tmdb_smoke -- --ignored --exact
```

Do not enable live tests in the default workspace gate. They validate deployment credentials/connectivity, not deterministic parsing contracts. Never print captured responses containing secrets, and do not commit newly recorded payloads without sanitizing and reviewing them.

The ignored server Web test is not an online test; it requires a prior production Web build:

```bash
pnpm --dir web build
cargo test -p fixer-server --test web -- --ignored
```

## Adding providers and writers

A provider change should include:

1. a typed `ProviderDescriptor` with stable ID, media capabilities, and correct network requirement;
2. validated configuration and redacted secrets;
3. local fixture tests for search, fetch, malformed response, and HTTP status handling;
4. SDK orchestration coverage where merge/provenance behavior changes;
5. updates to provider registration, `providers list`, and [provider documentation](providers.md).

A writer/output change should include no-write plan tests, deterministic snapshots where useful, executor behavior tests for each operation, and updates to [output documentation](output.md). Keep network acquisition and archive/tag mutation as explicit separate intents unless the approval and rollback contract changes.

## Documentation checks

Task documentation verification uses:

```bash
cargo test --workspace --doc --locked
cargo run -q -p fixer-cli -- --help
for command in \
  "search" "search anime" "search book" "search movie" "search music" "search television" \
  "resolve" "resolve anime" "resolve book" "resolve movie" "resolve music" "resolve television" \
  "scan" "plan" "scrape" "config" "config validate" "providers" "providers list"
do
  set -- $command
  cargo run -q -p fixer-cli -- "$@" --help >/dev/null
done
git diff --check
```

Check relative Markdown targets and validate every documented CLI command against current Clap output. Update documentation in the same commit as a public flag, environment variable, JSON schema, provider, writer, or security-boundary change.
