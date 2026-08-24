# Fixer Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a local-first, multi-source metadata scraper for movies, television, anime, music, and books, exposed through an ergonomic Rust SDK, a Clap CLI, an Axum service, and a SolidJS 2 web workspace.

**Architecture:** Use a layered Cargo workspace. `fixer-core` owns runtime-independent domain types and protocols; `fixer-sdk` owns Tokio orchestration and ergonomic typed flows; providers and writers are compile-time crates; CLI and Server are thin application adapters. Search, matching, fetching, merging, planning, and execution remain separate, and all filesystem mutations pass through an inspectable `OutputPlan`.

**Tech Stack:** Rust 2024, Tokio, Serde, Reqwest/Rustls, Clap, Axum, SQLite, SolidJS 2.0+, TypeScript, Tailwind CSS, TanStack Router, TanStack Query, Vite.

---

## Execution rules

- Work directly on `main`; do not create a branch or worktree.
- Each task below ends in one focused commit. If a task grows beyond its listed files or acceptance criteria, split it before continuing.
- Do not commit `.pi/`, `.pi-glla/`, secrets, generated databases, provider response caches, `target/`, `node_modules/`, or Web build output.
- Preserve a buildable `main` after every commit.
- Use focused checks during a task. Run broader crate tests only at vertical-slice boundaries and the full workspace suite only at phase gates or final delivery.
- Provider contract tests use checked-in sanitized fixtures. Real-network tests are ignored by default and never gate normal development.
- Before adding each dependency or using a framework API, verify its current official documentation. Pin compatible versions in workspace manifests and lockfiles.
- Public SDK types and methods require rustdoc examples. Public API changes must include compile-time/API tests.

## Phase 1 — Repository and core contracts

### Task 1: Convert the starter package into a clean workspace

**Files:**

- Modify: `.gitignore`
- Modify: `Cargo.toml`
- Delete: `src/main.rs`
- Create: `README.md`
- Create: `crates/fixer-core/Cargo.toml`
- Create: `crates/fixer-core/src/lib.rs`

**Steps:**

1. Replace the root package with a virtual Cargo workspace using resolver `3`; initially include only `crates/fixer-core`.
2. Add shared package metadata and centralize versions for foundational dependencies such as `serde`, `serde_json`, `thiserror`, and the selected BCP 47 language-tag crate.
3. Expand `.gitignore` for Pi state, Rust build output, environment files, SQLite files, provider caches, Node dependencies, and Web build output.
4. Add a minimal `fixer-core` library with crate-level documentation and `#![forbid(unsafe_code)]`.
5. Add a README that states the product scope, local-first model, supported media domains, and current development status without claiming unimplemented features.
6. Run `cargo check -p fixer-core`; expect success.
7. Mechanically confirm `git status --short` contains no `.pi/` or `.pi-glla/` entries.
8. Commit:

```bash
git add .gitignore Cargo.toml Cargo.lock README.md crates/fixer-core src/main.rs
git commit -m "chore: initialize fixer workspace"
```

### Task 2: Add localized values, confidence, identifiers, and provenance

**Files:**

- Create: `crates/fixer-core/src/locale.rs`
- Create: `crates/fixer-core/src/identity.rs`
- Create: `crates/fixer-core/src/confidence.rs`
- Create: `crates/fixer-core/src/provenance.rs`
- Create: `crates/fixer-core/src/error.rs`
- Create: `crates/fixer-core/tests/primitives.rs`
- Modify: `crates/fixer-core/src/lib.rs`

**Required public surface:**

```rust
pub struct LanguageTag(/* validated BCP 47 representation */);
pub struct LocalizedValue<T> { /* all tagged and untagged values */ }
pub struct LocalePolicy { /* ordered preferred tags and fallback behavior */ }
pub struct Confidence(/* finite value in 0.0..=1.0 */);
pub struct ProviderId(/* validated stable identifier */);
pub struct ExternalId { pub namespace: String, pub value: String }
pub struct SourceRef { /* provider, external id, locale, observed time */ }
pub struct Sourced<T> { pub value: T, pub source: SourceRef, pub confidence: Confidence }
pub struct ProvenanceMap(/* field path to one or more sources */);
```

**Steps:**

1. Write focused tests for valid/invalid confidence values, BCP 47 parsing, exact locale selection, parent-language fallback, `und` fallback, external-ID equality, and provenance lookup.
2. Run `cargo test -p fixer-core --test primitives`; expect failures because the modules do not exist.
3. Implement only the primitives needed by the tests. Preserve original strings while using normalized forms for lookup.
4. Add Serde support and structured construction errors; avoid panicking constructors in public APIs.
5. Re-export the intended public types from `lib.rs`.
6. Run `cargo test -p fixer-core --test primitives`; expect all tests to pass.
7. Commit:

```bash
git add crates/fixer-core
git commit -m "feat(core): add localized metadata primitives"
```

### Task 3: Model Work, Release, Asset, and five media domains

**Files:**

- Create: `crates/fixer-core/src/media/mod.rs`
- Create: `crates/fixer-core/src/media/common.rs`
- Create: `crates/fixer-core/src/media/movie.rs`
- Create: `crates/fixer-core/src/media/television.rs`
- Create: `crates/fixer-core/src/media/anime.rs`
- Create: `crates/fixer-core/src/media/music.rs`
- Create: `crates/fixer-core/src/media/book.rs`
- Create: `crates/fixer-core/tests/media_models.rs`
- Modify: `crates/fixer-core/src/lib.rs`

**Required model boundaries:**

- Common value objects: titles, summaries, dates, people/credits, artwork references, ratings, genres, content ratings, and runtime/duration.
- `WorkId`, `ReleaseId`, and `AssetId` are distinct newtypes.
- Movie: work plus typed releases/editions.
- Television: series, season, episode, and ordering scheme.
- Anime: series/work relation, cour/season, OVA/special classification, aired and absolute numbering.
- Music: artist, release group/album, release, disc, and track.
- Book: work, edition, contributors, ISBN-10/ISBN-13, publisher, and file asset.
- Asset: source path and typed local-file facts; no filesystem access occurs in the model.

**Steps:**

1. Write construction and Serde round-trip tests using one representative entity per media domain.
2. Add compile-fail coverage (for example with `trybuild`) proving that a book ISBN cannot be supplied to a movie-specific constructor and an episode sequence is not a track sequence.
3. Run the focused model tests; expect failure before implementation.
4. Implement common value objects and media-specific modules. Prefer small newtypes and domain enums over free-form strings where semantics are stable.
5. Use non-exhaustive public enums only where downstream matching would otherwise be brittle; do not add generic “unknown JSON” bags to domain structs.
6. Run `cargo test -p fixer-core --test media_models`; run compile-fail tests once after snapshots are accepted.
7. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-core
git commit -m "feat(core): model supported media domains"
```

### Task 4: Define provider, HTTP, writer, and output-plan protocols

**Files:**

- Create: `crates/fixer-core/src/provider.rs`
- Create: `crates/fixer-core/src/http.rs`
- Create: `crates/fixer-core/src/output/mod.rs`
- Create: `crates/fixer-core/src/output/plan.rs`
- Create: `crates/fixer-core/src/output/writer.rs`
- Create: `crates/fixer-core/tests/contracts.rs`
- Modify: `crates/fixer-core/src/lib.rs`

**Required contracts:**

```rust
pub trait Provider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;
    fn search<'a>(&'a self, request: SearchRequest, http: &'a dyn HttpClient)
        -> BoxFuture<'a, Result<Vec<Candidate>, ProviderError>>;
    fn fetch<'a>(&'a self, request: FetchRequest, http: &'a dyn HttpClient)
        -> BoxFuture<'a, Result<MetadataDocument, ProviderError>>;
}

pub trait HttpClient: Send + Sync {
    fn execute<'a>(&'a self, request: HttpRequest)
        -> BoxFuture<'a, Result<HttpResponse, HttpError>>;
}

pub trait Writer: Send + Sync {
    fn plan(&self, request: WriteRequest) -> Result<OutputPlan, PlanningError>;
}
```

`SearchRequest`, `Candidate`, and `MetadataDocument` use typed media variants. Contracts must be object-safe so applications can register heterogeneous providers at compile time.

`OutputPlan` operations initially represent create-directory, write-bytes, copy, symlink, hardlink, and reflink. Downloaded resources are represented as planned content supplied before execution, not as hidden Writer-side network calls.

**Steps:**

1. Write compile-time contract tests with a fake provider, fake HTTP client, and fake writer stored behind `Arc<dyn Trait>`.
2. Test Provider capability filtering and structured unsupported-media errors.
3. Test `OutputPlan` serialization and ensure every operation contains source/target information required for preview.
4. Implement boxed, runtime-neutral futures using `std::future::Future` and `std::pin::Pin`; do not add Tokio to `fixer-core`.
5. Run `cargo test -p fixer-core --test contracts`; expect pass.
6. Run `cargo tree -p fixer-core` and confirm Tokio, Axum, Clap, Reqwest, and SQLite are absent.
7. Commit:

```bash
git add crates/fixer-core Cargo.toml Cargo.lock
git commit -m "feat(core): define scraper extension contracts"
```

### Task 5: Implement explainable matching and field-level merging

**Files:**

- Create: `crates/fixer-core/src/matching/mod.rs`
- Create: `crates/fixer-core/src/matching/score.rs`
- Create: `crates/fixer-core/src/merge/mod.rs`
- Create: `crates/fixer-core/src/merge/policy.rs`
- Create: `crates/fixer-core/src/resolved.rs`
- Create: `crates/fixer-core/tests/matching.rs`
- Create: `crates/fixer-core/tests/merge.rs`
- Modify: `crates/fixer-core/src/lib.rs`

**Behavior:**

- Exact external IDs outrank all fuzzy evidence.
- Matching exposes positive and negative evidence for title, aliases, year/date, and domain sequence identifiers.
- Merge policy resolves provider order globally, per media kind, and per field path.
- Localized values are merged without discarding alternate languages.
- Ratings remain separated by rating system.
- Credits and artwork deduplicate by stable IDs first and normalized identity second.
- A resolved result carries value, provenance, conflicts, completeness, and warnings.

**Steps:**

1. Add table-driven matcher tests for exact ID, exact localized title, alias, year mismatch, and ambiguous candidates.
2. Add merge tests using two movie documents with complementary locales, conflicting summaries, duplicate people, distinct ratings, and artwork.
3. Verify the tests fail before implementation.
4. Implement a deterministic baseline scorer; do not add machine learning or provider-specific hidden weights.
5. Implement field-path policy lookup and typed media merge entry points. Keep unsupported merge combinations explicit.
6. Run only `cargo test -p fixer-core --test matching --test merge`; expect pass.
7. Run the Phase 1 gate: `cargo test -p fixer-core`; expect pass.
8. Commit:

```bash
git add crates/fixer-core
git commit -m "feat(core): add matching and metadata merging"
```

## Phase 2 — Ergonomic SDK, local sources, and safe output

### Task 6: Add the Tokio SDK and fixture provider

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-sdk/Cargo.toml`
- Create: `crates/fixer-sdk/src/lib.rs`
- Create: `crates/fixer-sdk/src/builder.rs`
- Create: `crates/fixer-sdk/src/query/mod.rs`
- Create: `crates/fixer-sdk/src/query/movie.rs`
- Create: `crates/fixer-sdk/src/orchestrator.rs`
- Create: `crates/fixer-sdk/src/fixture.rs`
- Create: `crates/fixer-sdk/tests/movie_flow.rs`

**Ergonomic acceptance API:**

```rust
let fixer = Fixer::builder()
    .provider(FixtureProvider::new(documents))
    .preferred_languages(["zh-CN", "zh-TW", "en"])?
    .build()?;

let outcome = fixer.movie("花样年华").year(2000).resolve().await?;
assert_eq!(outcome.value().release_year(), Some(2000));
```

**Steps:**

1. Add an SDK integration test for the API above, two concurrently searched fixture providers, deterministic ranking, fetch, merge, warning collection, and typed `Resolved<Movie>` output.
2. Add builder validation tests for no providers, invalid language tags, and duplicate Provider IDs.
3. Verify tests fail before implementation.
4. Implement `FixerBuilder`, typed movie query builder, orchestrator, and fixture provider. Tokio belongs here, not in Core.
5. Make `.resolve()` the simple path; retain lower-level `.search()`, candidate selection, and `.fetch_selected()` without requiring them for normal usage.
6. Add rustdoc examples for builder and movie query.
7. Run `cargo test -p fixer-sdk`; expect pass.
8. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-sdk
git commit -m "feat(sdk): add ergonomic movie resolution flow"
```

### Task 7: Implement the default HTTP client and minimal network model

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-http/Cargo.toml`
- Create: `crates/fixer-http/src/lib.rs`
- Create: `crates/fixer-http/src/client.rs`
- Create: `crates/fixer-http/src/config.rs`
- Create: `crates/fixer-http/tests/client.rs`
- Modify: `crates/fixer-sdk/src/builder.rs`
- Modify: `crates/fixer-sdk/src/lib.rs`

**Behavior:**

- Rustls-backed HTTPS by default.
- Standard proxy environment variables are respected.
- One optional explicit global HTTP/SOCKS proxy.
- Timeout and user-agent have safe defaults.
- Offline mode prevents requests before the client is invoked.
- Provider endpoint override remains provider configuration, not HTTP-client routing.
- Request/response DTOs redact authorization, API keys, cookies, and proxy credentials in Debug/log output.

**Steps:**

1. Add local mock-server tests for successful GET, timeout, non-success status, explicit proxy parsing, and header redaction. Do not call the public internet.
2. Add an SDK test proving `.offline()` skips network-only providers and returns local results plus a structured warning.
3. Verify focused tests fail.
4. Implement `ReqwestHttpClient` and ergonomic SDK builder hooks: `.proxy(...)`, `.timeout(...)`, `.http_client(...)`.
5. Run `cargo test -p fixer-http -p fixer-sdk`; expect pass.
6. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-http crates/fixer-sdk
git commit -m "feat(http): add configurable default transport"
```

### Task 8: Add local file identification and local metadata Provider

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-local/Cargo.toml`
- Create: `crates/fixer-provider-local/src/lib.rs`
- Create: `crates/fixer-provider-local/src/identify.rs`
- Create: `crates/fixer-provider-local/src/nfo.rs`
- Create: `crates/fixer-provider-local/src/json.rs`
- Create: `crates/fixer-provider-local/tests/fixtures/movie.nfo`
- Create: `crates/fixer-provider-local/tests/fixtures/movie.json`
- Create: `crates/fixer-provider-local/tests/identify.rs`
- Create: `crates/fixer-provider-local/tests/provider.rs`

**Behavior:**

- Parse movie title/year from common file and directory names into evidence-bearing `MediaHint` values.
- Read sanitized local JSON and the supported subset of Kodi/Jellyfin NFO.
- Never follow symlinks outside the explicitly supplied scan root during recursive scanning.
- Local Provider declares no network requirement.
- Unsupported or malformed local files produce structured warnings with paths.

**Steps:**

1. Add tests for filename identification, nested movie directory, malformed year, NFO parsing, JSON parsing, and scan-root escape prevention.
2. Verify tests fail.
3. Implement the narrow movie subset first; do not implement all media tag parsers in this task.
4. Run `cargo test -p fixer-provider-local`; expect pass.
5. Add the local Provider to the SDK movie-flow integration test and run that one test.
6. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-provider-local crates/fixer-sdk
git commit -m "feat(local): identify movies and read sidecar metadata"
```

### Task 9: Add templates and local metadata writers

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-writer-local/Cargo.toml`
- Create: `crates/fixer-writer-local/src/lib.rs`
- Create: `crates/fixer-writer-local/src/path_template.rs`
- Create: `crates/fixer-writer-local/src/content_template.rs`
- Create: `crates/fixer-writer-local/src/json.rs`
- Create: `crates/fixer-writer-local/src/nfo.rs`
- Create: `crates/fixer-writer-local/src/manifest.rs`
- Create: `crates/fixer-writer-local/tests/writers.rs`
- Create: `crates/fixer-writer-local/tests/snapshots/`

**Behavior:**

- Path templates expose a documented allowlist of variables and filters.
- Template expansion rejects absolute paths, parent traversal, NUL, and platform-invalid output.
- Built-in movie JSON and NFO writers produce deterministic output.
- Manifest records final metadata field sources and planned files but never secrets.
- Writers only plan operations; they do not touch the filesystem.

**Steps:**

1. Add failing tests for safe path rendering, traversal rejection, missing variables, locale projection, deterministic JSON, NFO snapshots, and provenance manifest.
2. Implement the smallest template surface needed by built-in presets; do not expose arbitrary code execution.
3. Run `cargo test -p fixer-writer-local`; review and accept snapshots intentionally.
4. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-writer-local
git commit -m "feat(writer): plan templated movie metadata output"
```

### Task 10: Execute safe output plans and media placement

**Files:**

- Create: `crates/fixer-sdk/src/output/mod.rs`
- Create: `crates/fixer-sdk/src/output/executor.rs`
- Create: `crates/fixer-sdk/src/output/fingerprint.rs`
- Create: `crates/fixer-sdk/tests/output_execution.rs`
- Modify: `crates/fixer-sdk/src/lib.rs`

**Behavior:**

- `in-place`, relative/absolute symlink, hardlink, copy, and reflink are explicit modes.
- `move` does not exist.
- Existing targets default to `NoOverwrite`.
- Reflink has `Required` and `FallbackToCopy`; fallback is never implicit.
- Plans fingerprint relevant source/target state and reject stale execution.
- Text/binary writes and copies use same-directory temporary files followed by atomic rename where the platform supports it.
- Partial failure returns an operation report and cleans temporary artifacts.

**Steps:**

1. Add temporary-directory tests for dry-run, no-overwrite, path traversal, stale plan, copy, symlink, and hardlink.
2. Add platform-gated Reflink tests; unsupported filesystems must yield a precise skipped/unsupported result rather than making the suite flaky.
3. Verify tests fail before implementation.
4. Implement the executor and expose `plan.preview()` plus `plan.execute(policy)` through the SDK.
5. Run `cargo test -p fixer-sdk --test output_execution`; expect pass.
6. Run the Phase 2 gate: `cargo test -p fixer-core -p fixer-sdk -p fixer-http -p fixer-provider-local -p fixer-writer-local`.
7. Commit:

```bash
git add crates/fixer-sdk
git commit -m "feat(sdk): execute safe output plans"
```

## Phase 3 — Movie CLI and first network Provider

### Task 11: Deliver the movie CLI vertical slice

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-cli/Cargo.toml`
- Create: `crates/fixer-cli/src/main.rs`
- Create: `crates/fixer-cli/src/args.rs`
- Create: `crates/fixer-cli/src/config.rs`
- Create: `crates/fixer-cli/src/commands/mod.rs`
- Create: `crates/fixer-cli/src/commands/search.rs`
- Create: `crates/fixer-cli/src/commands/resolve.rs`
- Create: `crates/fixer-cli/src/commands/scrape.rs`
- Create: `crates/fixer-cli/src/render.rs`
- Create: `crates/fixer-cli/tests/cli.rs`

**First CLI surface:**

```text
fixer search movie <TITLE> [--year YEAR]
fixer resolve movie <TITLE> [--year YEAR] [--json]
fixer scrape <PATH> --kind movie [--dry-run|--apply]
             [--placement in-place|symlink|hardlink|copy|reflink]
             [--offline] [--proxy URL]
fixer config validate
fixer providers list
```

**Steps:**

1. Add CLI tests for help, invalid placement, mutually exclusive dry-run/apply, JSON result shape, partial-success exit code, and default no-overwrite behavior.
2. Implement configuration precedence: flags, environment, configuration file, defaults. Secrets are not printed by `config validate`.
3. Wire the local Provider and writer into direct SDK execution; do not require a running Server.
4. Use stable JSON DTOs rather than directly serializing internal SDK structs.
5. Run `cargo test -p fixer-cli`; manually probe `cargo run -p fixer-cli -- --help`.
6. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-cli
git commit -m "feat(cli): deliver local movie scraping flow"
```

### Task 12: Add TMDB and verify real multi-source movie merging

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-tmdb/Cargo.toml`
- Create: `crates/fixer-provider-tmdb/src/lib.rs`
- Create: `crates/fixer-provider-tmdb/src/config.rs`
- Create: `crates/fixer-provider-tmdb/src/movie.rs`
- Create: `crates/fixer-provider-tmdb/src/error.rs`
- Create: `crates/fixer-provider-tmdb/tests/fixtures/search_movie.json`
- Create: `crates/fixer-provider-tmdb/tests/fixtures/movie_details.json`
- Create: `crates/fixer-provider-tmdb/tests/movie.rs`
- Modify: `crates/fixer-cli/src/config.rs`
- Modify: `crates/fixer-cli/src/main.rs`
- Modify: `crates/fixer-sdk/tests/movie_flow.rs`

**Behavior:**

- API token comes from explicit config or environment; never appears in Debug output.
- Base endpoint is overridable.
- Search and fetch pass requested locale where TMDB supports it, while retaining returned language facts.
- 401, 404, 429, malformed response, timeout, and empty results map to distinct errors/warnings.
- A local sidecar plus TMDB resolves into one movie with complementary fields and provenance.

**Steps:**

1. Add fixture-backed request/response contract tests with a local mock server.
2. Add a multi-source SDK test: local Chinese title/summary wins configured fields; TMDB supplies IDs, date, credits, and artwork; provenance identifies each source.
3. Implement TMDB movie search/fetch and CLI registration.
4. Add one ignored opt-in smoke test requiring `TMDB_API_TOKEN`; do not run it by default.
5. Run `cargo test -p fixer-provider-tmdb -p fixer-sdk -p fixer-cli`.
6. Commit:

```bash
git add Cargo.toml Cargo.lock crates/fixer-provider-tmdb crates/fixer-sdk crates/fixer-cli
git commit -m "feat(tmdb): add multi-source movie metadata"
```

## Phase 4 — Remaining media vertical slices

Each task extends existing common contracts rather than copying the movie pipeline. A media slice is complete only when explicit query, local identification/metadata, matching, merge, output planning, SDK API, and CLI entry point work together.

### Task 13: Add television series, seasons, and episodes

**Files:**

- Create: `crates/fixer-sdk/src/query/television.rs`
- Create: `crates/fixer-provider-local/src/television.rs`
- Create: `crates/fixer-provider-tmdb/src/television.rs`
- Create: `crates/fixer-writer-local/src/television.rs`
- Create: `crates/fixer-sdk/tests/television_flow.rs`
- Create: `crates/fixer-provider-tmdb/tests/fixtures/tv_*.json`
- Modify: `crates/fixer-cli/src/args.rs`
- Modify: `crates/fixer-cli/src/commands/`

**Acceptance:**

- Recognize `S01E02`, season folders, specials, and explicit external IDs.
- Preserve series/season/episode hierarchy and selected ordering scheme.
- Produce series, season, and episode NFO plans with artwork paths.
- Merge local episode facts with TMDB series/episode metadata.

**Verification:** Run focused tests for the four touched crates, then commit:

```bash
git commit -m "feat(tv): add series and episode scraping flow"
```

### Task 14: Add anime with Bangumi as the primary network Provider

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-bangumi/Cargo.toml`
- Create: `crates/fixer-provider-bangumi/src/lib.rs`
- Create: `crates/fixer-provider-bangumi/src/anime.rs`
- Create: `crates/fixer-provider-bangumi/tests/fixtures/`
- Create: `crates/fixer-provider-bangumi/tests/anime.rs`
- Create: `crates/fixer-sdk/src/query/anime.rs`
- Create: `crates/fixer-provider-local/src/anime.rs`
- Create: `crates/fixer-writer-local/src/anime.rs`
- Create: `crates/fixer-sdk/tests/anime_flow.rs`
- Modify: `crates/fixer-cli/src/`

**Acceptance:**

- Model and match original/Japanese, simplified Chinese, traditional Chinese, and English titles.
- Handle cour/season, OVA, ONA, special, aired numbering, and absolute numbering without flattening them.
- Support endpoint override and structured network failures.
- Offline local anime flow remains functional when Bangumi is unreachable.

**Verification:** Fixture tests plus one SDK/CLI vertical test; commit:

```bash
git commit -m "feat(anime): add localized anime scraping flow"
```

### Task 15: Add optional AniList as a complementary anime Provider

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-anilist/Cargo.toml`
- Create: `crates/fixer-provider-anilist/src/lib.rs`
- Create: `crates/fixer-provider-anilist/src/graphql.rs`
- Create: `crates/fixer-provider-anilist/tests/fixtures/`
- Create: `crates/fixer-provider-anilist/tests/anime.rs`
- Modify: `crates/fixer-sdk/tests/anime_flow.rs`
- Modify: `crates/fixer-cli/src/config.rs`

**Acceptance:** AniList can be disabled without reducing basic anime functionality; when enabled, its artwork and alternate titles merge with Bangumi/local values and retain provenance. Network failure is a warning if required fields are already satisfied.

**Verification:** Fixture and multi-source anime tests; commit:

```bash
git commit -m "feat(anilist): add optional anime metadata source"
```

### Task 16: Add music metadata and MusicBrainz

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-musicbrainz/Cargo.toml`
- Create: `crates/fixer-provider-musicbrainz/src/lib.rs`
- Create: `crates/fixer-provider-musicbrainz/src/music.rs`
- Create: `crates/fixer-provider-musicbrainz/tests/fixtures/`
- Create: `crates/fixer-provider-local/src/music.rs`
- Create: `crates/fixer-writer-local/src/music.rs`
- Create: `crates/fixer-sdk/src/query/music.rs`
- Create: `crates/fixer-sdk/tests/music_flow.rs`
- Modify: `crates/fixer-cli/src/`

**Acceptance:**

- Read the selected baseline audio tags and CUE metadata without modifying files.
- Resolve artist, release group, release, disc, and track identities.
- Respect MusicBrainz user-agent and rate-limit requirements through Provider behavior.
- Plan JSON/manifest output and tag updates; tag mutation remains confirmation-gated.
- Hardlink warning explains that in-place tag mutation changes all hardlinked paths.

**Verification:** Fixture-backed Provider tests and one album vertical flow; commit:

```bash
git commit -m "feat(music): add album and track scraping flow"
```

### Task 17: Add book metadata and Open Library

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-provider-openlibrary/Cargo.toml`
- Create: `crates/fixer-provider-openlibrary/src/lib.rs`
- Create: `crates/fixer-provider-openlibrary/src/book.rs`
- Create: `crates/fixer-provider-openlibrary/tests/fixtures/`
- Create: `crates/fixer-provider-local/src/book.rs`
- Create: `crates/fixer-writer-local/src/book.rs`
- Create: `crates/fixer-sdk/src/query/book.rs`
- Create: `crates/fixer-sdk/tests/book_flow.rs`
- Modify: `crates/fixer-cli/src/`

**Acceptance:**

- Extract ISBN/title/author from the selected baseline EPUB/OPF metadata without altering the book.
- Distinguish a work from its editions.
- Prefer exact ISBN edition matches over fuzzy title matches.
- Plan OPF/JSON/manifest/cover output; EPUB internal mutation remains confirmation-gated.
- Endpoint override and offline local-only flow work.

**Verification:** Fixture-backed Provider tests and one edition vertical flow; commit:

```bash
git commit -m "feat(books): add work and edition scraping flow"
```

### Task 18: Complete the cross-media CLI and configuration schema

**Files:**

- Modify: `crates/fixer-cli/src/args.rs`
- Modify: `crates/fixer-cli/src/config.rs`
- Modify: `crates/fixer-cli/src/commands/`
- Create: `crates/fixer-cli/src/json.rs`
- Create: `crates/fixer-cli/tests/cross_media.rs`
- Create: `docs/configuration.md`
- Create: `docs/cli.md`

**Acceptance:**

- All five media types support search/resolve/scan/plan/scrape where meaningful.
- Config supports locale preference, global proxy, timeout, confidence thresholds, output preset, placement, conflict policy, Provider enablement, endpoint, and secret references.
- Exit codes distinguish complete success, partial success, review required, invalid input/config, and execution failure.
- JSON output is versioned and stable.

**Verification:** CLI tests and `cargo test --workspace` as the Phase 4 gate; commit:

```bash
git commit -m "feat(cli): complete cross-media workflows"
```

## Phase 5 — Axum service and persistent jobs

### Task 19: Create the Axum service and versioned API DTOs

**Files:**

- Modify: `Cargo.toml`
- Create: `crates/fixer-server/Cargo.toml`
- Create: `crates/fixer-server/src/main.rs`
- Create: `crates/fixer-server/src/lib.rs`
- Create: `crates/fixer-server/src/app.rs`
- Create: `crates/fixer-server/src/api/mod.rs`
- Create: `crates/fixer-server/src/api/v1/mod.rs`
- Create: `crates/fixer-server/src/api/v1/health.rs`
- Create: `crates/fixer-server/src/api/v1/providers.rs`
- Create: `crates/fixer-server/src/api/error.rs`
- Create: `crates/fixer-server/tests/api.rs`

**Acceptance:**

- `/api/v1/health` and `/api/v1/providers` return stable DTOs.
- API errors include code, safe message, optional field details, and request ID.
- Binding defaults to `127.0.0.1`.
- Server startup rejects unauthenticated non-loopback configuration even before full auth is added.
- Server DTOs do not directly expose Core internal serialization.

**Verification:** Axum router tests without opening a public port; commit:

```bash
git commit -m "feat(server): add versioned axum API"
```

### Task 20: Add SQLite job persistence and migrations

**Files:**

- Create: `crates/fixer-server/migrations/0001_jobs.sql`
- Create: `crates/fixer-server/src/store/mod.rs`
- Create: `crates/fixer-server/src/store/sqlite.rs`
- Create: `crates/fixer-server/src/jobs/model.rs`
- Create: `crates/fixer-server/tests/store.rs`
- Modify: `crates/fixer-server/src/lib.rs`

**Acceptance:**

- Persist job input DTO, state, progress summary, timestamps, review summary, plan summary, and execution summary.
- Enforce valid state transitions in application code and compare-and-set database updates.
- On startup, running states become `interrupted`; queued work remains queued.
- SQLite does not store media binaries, Provider secrets, or Rust type snapshots.

**Verification:** Migration and restart-state tests using temporary databases; commit:

```bash
git commit -m "feat(server): persist scraper jobs in sqlite"
```

### Task 21: Add job workers, review actions, SSE, cancellation, and idempotency

**Files:**

- Create: `crates/fixer-server/src/jobs/mod.rs`
- Create: `crates/fixer-server/src/jobs/worker.rs`
- Create: `crates/fixer-server/src/jobs/events.rs`
- Create: `crates/fixer-server/src/api/v1/jobs.rs`
- Create: `crates/fixer-server/tests/jobs.rs`
- Modify: `crates/fixer-server/src/app.rs`

**Acceptance:**

- `POST /api/v1/jobs` returns `202` and an ID immediately.
- Fixed-count Tokio workers call SDK flows.
- SSE emits state/progress/review/completion events and supports reconnect from a bounded event history.
- Review endpoint selects candidates or resolves field conflicts before planning.
- Execute endpoint requires explicit approval and an idempotency key.
- Cancellation is cooperative between stages; no task is force-aborted during an atomic file replacement.

**Verification:** State-machine/API integration tests with Fixture Provider; commit:

```bash
git commit -m "feat(server): run persistent scraping jobs"
```

### Task 22: Add single-user authentication and filesystem boundaries

**Files:**

- Create: `crates/fixer-server/migrations/0002_auth.sql`
- Create: `crates/fixer-server/src/auth/mod.rs`
- Create: `crates/fixer-server/src/auth/password.rs`
- Create: `crates/fixer-server/src/auth/token.rs`
- Create: `crates/fixer-server/src/auth/session.rs`
- Create: `crates/fixer-server/src/fs_policy.rs`
- Create: `crates/fixer-server/tests/auth.rs`
- Create: `crates/fixer-server/tests/fs_policy.rs`
- Modify: `crates/fixer-server/src/app.rs`

**Acceptance:**

- Password hashes use a current memory-hard algorithm and calibrated safe defaults.
- API tokens are shown once and only digests are stored.
- Session cookies use HttpOnly and SameSite; Secure is required under configured HTTPS termination.
- Cookie-authenticated state changes enforce CSRF protection.
- Trusted proxy identity/header behavior is off unless exact trusted proxy ranges and headers are configured.
- All browsable/readable/writable media paths must remain under configured canonical roots, with symlink escape tests.

**Verification:** Focused auth and path-policy tests, then `cargo test -p fixer-server`; commit:

```bash
git commit -m "feat(server): secure local web access"
```

## Phase 6 — SolidJS Web workspace

### Task 23: Scaffold the Web application and typed API client

**Files:**

- Create: `web/package.json`
- Create: `web/pnpm-lock.yaml`
- Create: `web/tsconfig.json`
- Create: `web/vite.config.ts`
- Create: `web/src/index.tsx`
- Create: `web/src/app.tsx`
- Create: `web/src/styles.css`
- Create: `web/src/router.tsx`
- Create: `web/src/routes/__root.tsx`
- Create: `web/src/routes/index.tsx`
- Create: `web/src/lib/api.ts`
- Create: `web/src/lib/query-client.ts`
- Create: `web/src/components/app-shell.tsx`
- Create: `web/src/test/setup.ts`

**Steps:**

1. Confirm from official package documentation that the selected SolidJS release is stable and `>=2.0`; pin it and compatible TanStack Solid Router/Query, Tailwind, Vite, TypeScript, and test packages.
2. Scaffold the app with route generation/checking enabled and a typed client for `/api/v1` DTOs.
3. Establish accessible app-shell semantics, keyboard focus, responsive navigation, empty/loading/error states, and a small token layer in Tailwind/CSS rather than scattered arbitrary styles.
4. Add smoke tests for app mount, routing, API error display, and keyboard navigation.
5. Run `pnpm --dir web test` and `pnpm --dir web build`; expect pass.
6. Commit:

```bash
git add web .gitignore
git commit -m "feat(web): scaffold solid task workspace"
```

### Task 24: Build jobs, progress, review, and output-plan workflows

**Files:**

- Create: `web/src/routes/jobs/index.tsx`
- Create: `web/src/routes/jobs/$jobId.tsx`
- Create: `web/src/routes/jobs/$jobId.review.tsx`
- Create: `web/src/routes/jobs/$jobId.plan.tsx`
- Create: `web/src/components/job-status.tsx`
- Create: `web/src/components/progress-timeline.tsx`
- Create: `web/src/components/candidate-picker.tsx`
- Create: `web/src/components/field-conflict.tsx`
- Create: `web/src/components/output-diff.tsx`
- Create: `web/src/lib/sse.ts`
- Create: `web/src/routes/jobs/jobs.test.tsx`

**Acceptance:**

- List/filter jobs and create a scrape job.
- Reconnect SSE and reconcile events with fetched job state.
- Display Provider warnings without reducing partial success to generic failure.
- Compare candidates with matching evidence.
- Resolve field conflicts with locale/source visibility.
- Preview create/write/link/copy/Reflink operations and require explicit approval before execute.
- Cancellation and retries communicate exact consequences.

**Verification:** Component/integration tests using mocked API/SSE; one phase-end browser test against the real Axum service. Commit:

```bash
git commit -m "feat(web): add scraping review workflow"
```

### Task 25: Add search, library scanning, providers, settings, and templates

**Files:**

- Create: `web/src/routes/search.tsx`
- Create: `web/src/routes/library.tsx`
- Create: `web/src/routes/providers.tsx`
- Create: `web/src/routes/settings.tsx`
- Create: `web/src/routes/templates.tsx`
- Create: `web/src/components/locale-policy-editor.tsx`
- Create: `web/src/components/provider-status.tsx`
- Create: `web/src/components/template-preview.tsx`
- Modify: `crates/fixer-server/src/api/v1/`
- Modify: `crates/fixer-server/src/app.rs`

**Acceptance:**

- Search all supported media types.
- Browse only configured server roots; the browser cannot submit arbitrary host paths.
- Configure non-secret locale, proxy, thresholds, Provider enablement/endpoint, merge policy, output preset, and placement.
- Secret fields are write-only and never returned.
- Test Provider connectivity with precise but safe error categories.
- Validate templates and preview output paths/content without writing.

**Verification:** Web integration tests, Server API tests, `pnpm --dir web build`, and `cargo test -p fixer-server`; commit:

```bash
git commit -m "feat(web): add scraper configuration workspace"
```

### Task 26: Serve the built Web app from Axum

**Files:**

- Modify: `crates/fixer-server/Cargo.toml`
- Create: `crates/fixer-server/src/web.rs`
- Modify: `crates/fixer-server/src/app.rs`
- Modify: `crates/fixer-server/src/main.rs`
- Modify: `web/vite.config.ts`
- Create: `crates/fixer-server/tests/web.rs`

**Acceptance:**

- Production binary serves versioned API and built static assets.
- Client-side routes fall back to `index.html` without swallowing `/api` 404s.
- Static assets have immutable cache headers when content-hashed; HTML does not.
- Development mode documents separate Vite/Axum commands and proxy configuration.

**Verification:** Build Web once, run Server integration tests against output, and directly probe `/`, a nested route, a hashed asset, and `/api/v1/health`. Commit:

```bash
git commit -m "feat(server): serve the solid web application"
```

## Phase 7 — Release hardening

### Task 27: Add end-to-end fixtures and acceptance tests

**Files:**

- Create: `tests/fixtures/library/`
- Create: `scripts/e2e-local.sh`
- Create: `crates/fixer-cli/tests/e2e_library.rs`
- Create: `crates/fixer-server/tests/e2e_jobs.rs`
- Create: `web/e2e/critical-flow.spec.ts`
- Modify: root `Cargo.toml`

**Scenarios:**

1. Entirely offline movie resolution from local NFO to dry-run plan.
2. Two-source movie merge with a fixture HTTP server and provenance.
3. Ambiguous anime enters review rather than auto-writing.
4. Exact ISBN book match outranks fuzzy title.
5. Music release preserves disc/track identities.
6. Provider timeout produces partial success.
7. No-overwrite protects an existing NFO.
8. Hardlink across filesystems fails clearly; Reflink fallback only occurs when selected.
9. Server restart marks an active job interrupted.
10. Web user reviews a candidate, previews operations, and approves a Fixture-backed write.

**Verification:** Run targeted E2E suites once, then the complete Rust workspace and Web test/build gates. Commit:

```bash
git commit -m "test: cover cross-interface scraping workflows"
```

### Task 28: Complete operator, SDK, and security documentation

**Files:**

- Modify: `README.md`
- Create: `docs/sdk.md`
- Create: `docs/providers.md`
- Create: `docs/output.md`
- Create: `docs/server.md`
- Create: `docs/security.md`
- Create: `docs/development.md`
- Create: `docs/troubleshooting.md`
- Create: `examples/sdk_movie.rs`

**Documentation must cover:**

- Minimal SDK examples and advanced injection points.
- Compile-time Provider authoring contract.
- Chinese-region connectivity: direct access, standard proxy variables, explicit global proxy, endpoint override, offline behavior, and honest limits.
- Output templates, provenance manifest, no-overwrite, dry-run, atomic writes, and placement semantics.
- Hardlink mutation caveat, symlink portability, and Reflink fallback behavior.
- Server bind/auth defaults, trusted proxy configuration, media-root allowlist, backup/restore, and SQLite location.
- CLI exit codes and stable JSON schema.
- Development commands and fixture/online-test policy.

**Verification:** Run rustdoc tests, validate all documented CLI commands against `--help`, and check internal links. Commit:

```bash
git commit -m "docs: add fixer usage and operations guides"
```

### Task 29: Final verification and release readiness

**Files:**

- Modify only files required by discovered verification failures.
- Create: `CHANGELOG.md`

**Steps:**

1. Mechanically verify all promised public crates, CLI commands, API routes, Web routes, Provider registrations, output presets, placement modes, and configuration entries exist.
2. Run formatting and lint gates:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

1. Run Rust tests and documentation tests:

```bash
cargo test --workspace --all-features
cargo test --doc --workspace
```

1. Run Web gates:

```bash
pnpm --dir web test
pnpm --dir web build
```

1. Run the bounded local E2E suite and the critical browser flow. Do not run ignored real-network tests unless credentials and explicit approval are present.
2. Build release binaries and directly probe CLI help, an offline local scrape dry-run, Server startup security defaults, health API, nested Web route, and one confirmed Fixture-backed write.
3. Inspect dependency licenses and known security advisories; fix actionable findings or document accepted risk.
4. Confirm logs and generated manifests contain no fixture secrets, authorization headers, cookies, proxy credentials, or local paths outside intended reports.
5. Update `CHANGELOG.md` with implemented features, known limitations, compatibility, and migration notes.
6. Commit only necessary release changes:

```bash
git add CHANGELOG.md <verified-fix-files>
git commit -m "chore: prepare fixer release"
```

## Completion evidence

Before claiming completion, report:

- exact commits delivered;
- focused and full verification commands run;
- public symbols, commands, routes, presets, and registrations mechanically checked;
- unsupported platforms or skipped capability tests;
- ignored online tests not run;
- remaining known limitations.

A successful build alone is not completion. The acceptance scenarios and direct behavioral probes must pass.
