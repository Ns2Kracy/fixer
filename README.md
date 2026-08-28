# Fixer

Fixer is an in-development, local-first metadata scraper for movies, television, anime, music, and books.

The Rust workspace separates runtime-independent domain contracts, SDK orchestration, compile-time providers and writers, and thin CLI/server adapters. Metadata resolution and filesystem changes remain inspectable before execution.

## Current status

The core, SDK, local and network providers, local writers, cross-media CLI, persistent Axum server, and Solid workspace are available.

## CLI quick start

```bash
cargo run -p fixer-cli -- --help
cargo run -p fixer-cli -- scan ./library --kind movie --json
cargo run -p fixer-cli -- --local-root ./library --offline resolve movie "Arrival" --json
cargo run -p fixer-cli -- --offline plan ./library/Arrival.mkv --kind movie --json
```

`plan` never writes. `scrape` also previews by default; pass `--apply` to execute an approved output plan.

See [CLI workflows](docs/cli.md) and [configuration](docs/configuration.md) for command coverage, exit codes, JSON contracts, provider settings, and safety limits.

## Web development

Run Axum and Vite in separate terminals. The server requires an authentication password and at least one existing absolute media root:

```bash
FIXER_SERVER_PASSWORD=local-development-password \
FIXER_SERVER_MEDIA_ROOTS=/absolute/path/to/media \
cargo run -p fixer-server
```

```bash
pnpm --dir web dev
```

Vite listens on `127.0.0.1:5173` and proxies `/api` to Axum at `127.0.0.1:3000`. Keep the browser on the Vite origin during development.

For the production filesystem layout, build the Web app before starting the server:

```bash
pnpm --dir web build
FIXER_SERVER_PASSWORD=change-me \
FIXER_SERVER_MEDIA_ROOTS=/absolute/path/to/media \
cargo run -p fixer-server
```

The server reads `web/dist` by default. Set `FIXER_WEB_ROOT` to an absolute build directory when running an installed binary or packaged deployment.
