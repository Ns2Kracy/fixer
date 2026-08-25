# Fixer

Fixer is an in-development, local-first metadata scraper for movies, television, anime, music, and books.

The project is designed as a layered Rust workspace: a runtime-independent core, an ergonomic SDK, compile-time provider and writer crates, and thin CLI and server adapters. Metadata resolution and filesystem changes will remain inspectable before execution.

## Status

Fixer is at an early implementation stage. The workspace and core domain contracts are currently being built; scraping providers, the CLI, server, and web workspace are not yet available.
