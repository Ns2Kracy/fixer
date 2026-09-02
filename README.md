# Fixer

Fixer is an in-development, local-first metadata scraper for movies, television, anime, music, and books.

The Rust workspace separates runtime-independent domain contracts, typed SDK orchestration, compile-time providers and writers, and thin CLI/server adapters. Metadata resolution and filesystem changes remain inspectable before execution.

## Current status

The core, SDK, local and network providers, local writers, cross-media CLI, persistent Axum server, and Solid workspace are available. Public provider availability and schemas can still change before a stable release; CLI JSON DTOs carry an explicit schema version.

## Requirements

- Rust 1.85 or newer
- Node.js 24.15 or newer for the Web application
- pnpm 11.22.0, pinned in `web/package.json`
- `curl` for the production browser acceptance harness

Install Web dependencies with:

```bash
pnpm --dir web install --frozen-lockfile
```

## CLI quick start

Run the development binary through Cargo:

```bash
cargo run -p fixer-cli -- --help
cargo run -p fixer-cli -- scan ./library --kind movie --json
cargo run -p fixer-cli -- --local-root ./library --offline resolve movie "Arrival" --json
cargo run -p fixer-cli -- --offline plan ./library/Arrival.mkv --kind movie --json
```

`plan` never writes. `scrape` also previews by default; pass `--apply` to execute the newly generated plan. Default execution refuses existing file-like targets.

See [CLI workflows](docs/cli.md) and [configuration](docs/configuration.md) for all commands, exit codes, JSON contracts, provider settings, and safety limits.

## SDK quick start

Run the registered offline example, which parses the committed movie NFO fixture and injects `LocalProvider`:

```bash
cargo run -p fixer-sdk --example sdk_movie
```

The [SDK guide](docs/sdk.md) covers typed resolution, explicit search/select/fetch, custom HTTP clients, provider precedence, offline behavior, output execution, and compile-time provider authoring.

## Docker quick start

Deploy the stable image in a new directory without cloning the repository:

```bash
mkdir -p fixer-deployment
cd fixer-deployment
curl --fail --location --output compose.yaml \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/compose.yaml
curl --fail --location --output .env.docker.example \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/.env.docker.example
cp .env.docker.example .env.docker
mkdir -p media
printf 'media path: %s\n' "$PWD/media"
$EDITOR .env.docker
docker compose --env-file .env.docker up -d --wait
```

Set `FIXER_MEDIA_PATH` to the printed absolute path before startup. Keep `FIXER_SERVER_ALLOWED_ORIGINS` equal to the exact URL you will open; the template uses `http://127.0.0.1:3000`. When the service is healthy, open that URL. A new database redirects unauthenticated visitors to `/login`, where **Sign up** creates the single administrator account; later visits use **Sign in** with that username and password.

`latest` is the stable channel and `edge` tracks `main`. Set `FIXER_IMAGE=ghcr.io/ns2kracy/fixer:0.1.0` to pin a release, or use `FIXER_IMAGE=ghcr.io/ns2kracy/fixer@sha256:<manifest-digest>` for an immutable deployment. See [Docker deployment](docs/server.md#docker-deployment) for image channels, permissions, persistence, source builds, upgrades, reverse proxies, and recovery.

## Server and Web development

Run Axum and Vite in separate terminals. The server requires at least one existing absolute media root and the exact Vite browser origin. Use a disposable database when testing first-run registration:

```bash
FIXER_SERVER_DATABASE='/tmp/fixer-development.sqlite3' \
FIXER_SERVER_MEDIA_ROOTS='/absolute/path/to/media' \
FIXER_SERVER_ALLOWED_ORIGINS='http://127.0.0.1:5173' \
cargo run -p fixer-server
```

```bash
pnpm --dir web dev
```

Vite listens on `127.0.0.1:5173` and proxies `/api` to Axum at `127.0.0.1:3000`. Keep the browser on the Vite origin during development.

For the production filesystem layout, build the Web app before starting the server:

```bash
pnpm --dir web build
FIXER_SERVER_MEDIA_ROOTS='/absolute/path/to/media' \
FIXER_SERVER_ALLOWED_ORIGINS='http://127.0.0.1:3000' \
cargo run -p fixer-server
```

The server reads `web/dist` by default. Set `FIXER_WEB_ROOT` to an absolute build directory when running an installed binary or packaged deployment. Read [server operations](docs/server.md) and the [security model](docs/security.md) before using a non-loopback listener or reverse proxy.

## Safety model

- Network providers are compile-time implementations with typed requests/responses and endpoint overrides.
- Offline mode skips network providers; it does not replay a response cache.
- Writers produce plans without I/O, and `scrape` requires `--apply` for mutation.
- SDK execution validates targets and stale state, defaults to no-overwrite, and publishes complete individual files where the filesystem supports the required operation.
- The server canonicalizes a required media-root allowlist and requires authentication, CSRF for browser writes, explicit review, and idempotency before execution.
- A multi-operation output plan is not transactional; inspect partial execution reports before retrying.

Placement and platform caveats are documented in [output planning and execution](docs/output.md).

## Documentation

| Guide | Contents |
| --- | --- |
| [CLI](docs/cli.md) | search/resolve/scan/plan/scrape workflows, media coverage, exit codes, stable JSON schema |
| [Configuration](docs/configuration.md) | file/environment/flag precedence, secrets, providers, placement, conflicts |
| [SDK](docs/sdk.md) | typed flows, injection points, errors, Provider contract, runnable example |
| [Providers](docs/providers.md) | capabilities, credentials, proxies, endpoint overrides, Chinese-region and offline limits |
| [Output](docs/output.md) | templates, manifests, dry-run, no-overwrite, publication/rollback limits, placement semantics |
| [Server](docs/server.md) | bind/auth defaults, reverse proxy, media roots, SQLite, restart, backup/restore |
| [Security](docs/security.md) | sessions, CSRF, API tokens, CORS, trusted proxies, filesystem boundaries, known limits |
| [Development](docs/development.md) | toolchains, test gates, fixtures, ignored online tests, browser acceptance |
| [Troubleshooting](docs/troubleshooting.md) | CLI/provider/output/server/auth/database/browser diagnosis and recovery |

## Project layout

```text
crates/fixer-core              domain and runtime-neutral contracts
crates/fixer-sdk               typed orchestration and output execution
crates/fixer-http              default rustls/Reqwest transport
crates/fixer-provider-*        local and network providers
crates/fixer-writer-local      planning-only local writers/templates
crates/fixer-cli               command-line application
crates/fixer-server            Axum, SQLite jobs, auth, static Web serving
web                            Solid application and browser tests
tests/fixtures/library         shared offline acceptance media
```

## Verification

Run the complete local gates before merging cross-cutting work:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --workspace --doc --locked
pnpm --dir web test
pnpm --dir web build
```

The real local browser flow is self-contained and may take longer than 60 seconds on a cold build cache:

```bash
scripts/e2e-local.sh
```

See [development](docs/development.md) for fixture and online-test policy.
