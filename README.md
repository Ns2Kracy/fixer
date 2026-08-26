# Fixer

Fixer is an in-development, local-first metadata scraper for movies, television, anime, music, and books.

The Rust workspace separates runtime-independent domain contracts, SDK orchestration, compile-time providers and writers, and a thin CLI adapter. Metadata resolution and filesystem changes remain inspectable before execution.

## Current status

The core, SDK, local and network provider slices, local writers, and cross-media CLI are available. The server and web workspace remain planned work.

## CLI quick start

```bash
cargo run -p fixer-cli -- --help
cargo run -p fixer-cli -- scan ./library --kind movie --json
cargo run -p fixer-cli -- --local-root ./library --offline resolve movie "Arrival" --json
cargo run -p fixer-cli -- --offline plan ./library/Arrival.mkv --kind movie --json
```

`plan` never writes. `scrape` also previews by default; pass `--apply` to execute an approved output plan.

See [CLI workflows](docs/cli.md) and [configuration](docs/configuration.md) for command coverage, exit codes, JSON contracts, provider settings, and safety limits.
