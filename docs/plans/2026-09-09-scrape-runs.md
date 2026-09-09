# Scrape Runs Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the exposed job/review/plan workflow with durable one-click scrape runs, exact provider-ID selection, immutable audit results, and safe correction runs.

**Architecture:** `fixer-core` owns deterministic categorical candidate ordering and exact provider targets; `fixer-sdk` owns automatic/exact scrape resolution and auditable output execution. `fixer-server` persists `ScrapeRun`, dispatches run IDs through a bounded Tokio `mpsc` channel to one worker, and exposes only scrape APIs. The Solid Web app starts runs and audits immutable results without exposing internal workflow stages.

**Tech Stack:** Rust 2024, Tokio, SQLx/SQLite, Axum, fixer-core, fixer-sdk, SolidJS, TanStack Solid Query/Router, Vitest, Playwright.

---

## Acceptance Ledger

- One authenticated `POST /api/v1/scrapes` authorizes and queues scanning, resolution, writing, and organization.
- Automatic selection always uses the deterministic first result and never blocks on confidence or a tied score.
- Exact TMDB movie/television targets bypass provider search and fetch the requested ID.
- SQLite stores immutable success/failure audit data and linked retry/correction history.
- Corrections overwrite only unchanged outputs produced by the previous successful run.
- Startup requeues interrupted work through Tokio `mpsc`; no custom queue or external broker is added.
- Folder ingestion creates scrape runs and has no needs-review state.
- Public server/Web contracts contain no Job, review, plan-confirmation, or confidence concepts.
- Existing job data migrates without silent loss, with incomplete legacy audits clearly marked.
- Core, SDK, server, Web unit tests and the real browser critical flow pass.

### Task 1: Replace confidence scoring with deterministic candidate ordering

**Files:**

- Modify: `crates/fixer-core/src/matching/mod.rs`
- Delete: `crates/fixer-core/src/matching/score.rs`
- Delete: `crates/fixer-core/src/confidence.rs`
- Modify: `crates/fixer-core/src/error.rs`
- Modify: `crates/fixer-core/src/lib.rs`
- Modify: `crates/fixer-core/tests/matching.rs`
- Modify: `crates/fixer-core/tests/primitives.rs`
- Modify: `crates/fixer-core/tests/contracts.rs`

**Step 1: Write failing matching tests**

Replace score assertions with behavior assertions covering:

```rust
#[test]
fn candidates_use_categorical_deterministic_order() {
    // exact external ID > exact title/year > exact title > provider order > native order
}

#[test]
fn equal_matches_keep_provider_and_native_order() {
    // Equal categorical matches are valid; no ambiguous result is returned.
}
```

Add a compile-level assertion that `RankedCandidate` no longer exposes `confidence`, and remove `Confidence` primitive tests.

**Step 2: Verify the tests fail**

Run: `cargo test -p fixer-core --test matching --test primitives --test contracts`

Expected: FAIL because current matching returns floating-point confidence and ambiguity.

**Step 3: Implement categorical ordering**

Introduce a private ordering key similar to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchClass {
    ProviderNative,
    ExactTitle,
    ExactTitleYear,
    ExactExternalId,
}
```

Preserve provider registration index and provider-native candidate index as stable tie-breakers. Keep exact external-ID grouping required by anime and television. Remove threshold and tied-top-score branches.

**Step 4: Remove confidence from the public core API**

Delete `Confidence`, `InvalidConfidence`, confidence evidence values, and stale exports. Keep useful categorical match evidence only if it is consumed by identity grouping or audit output.

**Step 5: Run focused and crate tests**

Run: `cargo test -p fixer-core`

Expected: PASS with no confidence or ambiguity behavior.

**Step 6: Commit**

```bash
git add crates/fixer-core
git commit -m "refactor(core): replace confidence with deterministic ordering"
```

### Task 2: Add exact provider targets and auditable operation reports

**Files:**

- Create: `crates/fixer-core/src/scrape.rs`
- Modify: `crates/fixer-core/src/provider.rs`
- Modify: `crates/fixer-core/src/output/mod.rs`
- Modify: `crates/fixer-core/src/output/plan.rs`
- Modify: `crates/fixer-core/src/output/writer.rs`
- Modify: `crates/fixer-core/src/lib.rs`
- Modify: `crates/fixer-core/src/error.rs`
- Create: `crates/fixer-core/tests/scrape.rs`
- Create: `crates/fixer-core/tests/execution_report.rs`

**Step 1: Write failing target validation tests**

Cover valid generic targets and TMDB-specific validation:

```rust
let target = ProviderTarget::new(
    MediaKind::Movie,
    ProviderId::new("tmdb")?,
    ExternalId::new("tmdb", "329865")?,
)?;
assert_eq!(target.external_id().value, "329865");
```

Reject namespace/provider mismatches, zero/negative/non-decimal TMDB IDs, and TMDB media kinds other than movie/television.

**Step 2: Write failing execution report tests**

Require a report entry for every attempted operation with operation kind, paths, outcome, and post-write fingerprint. Add replacement-manifest tests that accept an unchanged prior output and reject unknown or changed output files.

**Step 3: Verify the tests fail**

Run: `cargo test -p fixer-core --test scrape --test execution_report`

Expected: FAIL because target and report contracts do not exist.

**Step 4: Implement core contracts**

Add:

```rust
pub enum ScrapeSelection {
    Automatic,
    Exact(ProviderTarget),
}

pub struct OperationReport {
    pub operation_index: u64,
    pub kind: OutputOperationKind,
    pub source: Option<String>,
    pub destination: String,
    pub outcome: OperationOutcome,
    pub fingerprint: Option<String>,
}
```

Keep constructors validated and fields bounded. Add a replacement policy that requires exact destination and fingerprint matches from a prior report.

**Step 5: Run core tests and clippy**

Run: `cargo test -p fixer-core && cargo clippy -p fixer-core --all-targets -- -D warnings`

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-core
git commit -m "feat(core): add exact scrape targets and audit reports"
```

### Task 3: Build the SDK scrape workflow

**Files:**

- Create: `crates/fixer-sdk/src/scrape.rs`
- Modify: `crates/fixer-sdk/src/lib.rs`
- Modify: `crates/fixer-sdk/src/builder.rs`
- Modify: `crates/fixer-sdk/src/orchestrator.rs`
- Modify: `crates/fixer-sdk/src/query/movie.rs`
- Modify: `crates/fixer-sdk/src/query/television.rs`
- Modify: `crates/fixer-sdk/src/query/anime.rs`
- Modify: `crates/fixer-sdk/src/query/music.rs`
- Modify: `crates/fixer-sdk/src/query/book.rs`
- Modify: `crates/fixer-sdk/src/output/executor.rs`
- Modify: `crates/fixer-sdk/src/output/fingerprint.rs`
- Modify: `crates/fixer-sdk/src/output/mod.rs`
- Modify: `crates/fixer-sdk/src/error.rs` if present, otherwise the SDK error definition in `crates/fixer-sdk/src/lib.rs`
- Modify: `crates/fixer-sdk/tests/movie_flow.rs`
- Modify: `crates/fixer-sdk/tests/television_flow.rs`
- Modify: `crates/fixer-sdk/tests/anime_flow.rs`
- Modify: `crates/fixer-sdk/tests/output_execution.rs`
- Create: `crates/fixer-sdk/tests/scrape_flow.rs`

**Step 1: Write failing automatic-selection tests**

Use fixture providers with deliberately low/equal historical scores. Assert that the configured provider's first native result is selected and resolution completes without ambiguity or review.

**Step 2: Write failing exact-target tests**

Use an instrumented fixture provider and assert:

```rust
assert_eq!(provider.search_calls(), 0);
assert_eq!(provider.fetch_calls(), vec![ExternalId::new("tmdb", "329865")?]);
```

Cover movie and television, missing provider, unsupported media kind, malformed target, and provider fetch failure.

**Step 3: Verify the tests fail**

Run: `cargo test -p fixer-sdk --test scrape_flow --test movie_flow --test television_flow`

Expected: FAIL because the SDK has no first-class scrape or exact-fetch entry point.

**Step 4: Implement `Fixer::scrape`**

Add a builder that accepts scanned local metadata and `ScrapeSelection`. Automatic mode searches in configured provider order and resolves the first ordered identity group. Exact mode locates only the named provider and calls `Provider::fetch(FetchRequest)` without search. Both modes merge remote and local metadata and return selected identity, resolved media, warnings, and planner input.

**Step 5: Return detailed execution reports**

Update the SDK executor to populate every core `OperationReport`, calculate post-write fingerprints, retain partial results on failure, and enforce the correction replacement manifest before any overwrite.

**Step 6: Keep typed query APIs consistent**

Make existing movie, television, anime, music, and book `resolve` paths use the same deterministic orchestration. Do not duplicate provider selection rules in query modules.

**Step 7: Run SDK and dependent tests**

Run: `cargo test -p fixer-sdk && cargo clippy -p fixer-sdk --all-targets -- -D warnings`

Expected: PASS.

**Step 8: Commit**

```bash
git add crates/fixer-sdk
git commit -m "feat(sdk): add automatic and exact scrape workflows"
```

### Task 4: Remove confidence configuration end to end

**Files:**

- Modify: `crates/fixer-runtime/src/config.rs`
- Modify: `crates/fixer-runtime/src/runtime.rs`
- Modify: `crates/fixer-runtime/src/lib.rs`
- Modify: runtime configuration tests colocated in `crates/fixer-runtime/src/config.rs`
- Modify: `fixer.toml.example`
- Modify: `web/src/routes/settings.tsx`
- Modify: `web/src/routes/workspace.test.tsx`
- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/api.test.ts`

**Step 1: Write failing config tests**

Assert that the runtime configuration no longer serializes `review_confidence` or `auto_accept_confidence`. Add a targeted legacy-config test that reports the removed fields with an actionable migration message.

**Step 2: Verify failure**

Run: `cargo test -p fixer-runtime`

Expected: FAIL while confidence fields remain required and serialized.

**Step 3: Remove fields and validation**

Delete confidence fields, threshold relationship checks, defaults, snapshots, settings DTO fields, and Web controls. Keep provider order as the deterministic precedence setting.

**Step 4: Run runtime and Web API tests**

Run: `cargo test -p fixer-runtime && pnpm --dir web test -- lib/api.test.ts routes/workspace.test.tsx`

Expected: PASS and no confidence settings in requests or responses.

**Step 5: Commit**

```bash
git add crates/fixer-runtime fixer.toml.example web/src/routes/settings.tsx web/src/routes/workspace.test.tsx web/src/lib/api.ts web/src/lib/api.test.ts
git commit -m "refactor: remove confidence configuration"
```

### Task 5: Introduce the persisted `ScrapeRun` domain and migrate legacy jobs

**Files:**

- Create: `crates/fixer-server/src/scrapes/mod.rs`
- Create: `crates/fixer-server/src/scrapes/model.rs`
- Create: `crates/fixer-server/src/scrapes/artifacts.rs`
- Create: `crates/fixer-server/migrations/0008_scrape_runs.sql`
- Modify: `crates/fixer-server/src/store/mod.rs`
- Modify: `crates/fixer-server/src/store/sqlite.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Create: `crates/fixer-server/tests/scrape_model.rs`
- Create: `crates/fixer-server/tests/scrape_store.rs`
- Modify: `crates/fixer-server/tests/store.rs`

**Step 1: Write failing model tests**

Cover the four states, three run kinds, legal transitions, immutable terminal records, bounded progress/result/error JSON, and linked previous runs.

**Step 2: Write failing store and migration tests**

Create a pre-0008 database fixture containing completed, failed, queued, awaiting-confirmation, and writing jobs plus ingestion references. Open it through the store and assert:

- terminal rows map to succeeded/failed scrape runs;
- nonterminal rows map to `legacy_interrupted` failures;
- IDs and timestamps are retained;
- imported results have `legacy_incomplete: true`;
- ingestion references point to scrape runs;
- job tables no longer back runtime behavior.

**Step 3: Verify failure**

Run: `cargo test -p fixer-server --test scrape_model --test scrape_store --test store`

Expected: FAIL because the schema and model do not exist.

**Step 4: Implement migration and store**

Create `scrape_runs`, copy legacy data transactionally, rebuild `ingestion_sources` with `scrape_run_id`, add indexes for status/time and previous-run lookup, then remove obsolete job execution tables after successful copy.

Implement `ScrapeRunId`, input, progress, result, failure, list/get/insert/start/succeed/fail/requeue operations. Terminal updates must fail closed.

**Step 5: Run store tests**

Run: `cargo test -p fixer-server --test scrape_model --test scrape_store --test store`

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-server/src/scrapes crates/fixer-server/src/store crates/fixer-server/src/lib.rs crates/fixer-server/migrations/0008_scrape_runs.sql crates/fixer-server/tests
git commit -m "feat(server): persist immutable scrape runs"
```

### Task 6: Replace `JobRuntime` with the Tokio scrape queue and worker

**Files:**

- Create: `crates/fixer-server/src/scrapes/runtime.rs`
- Create: `crates/fixer-server/src/scrapes/worker.rs`
- Create: `crates/fixer-server/src/scrapes/events.rs`
- Modify: `crates/fixer-server/src/scrapes/mod.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Modify: `crates/fixer-server/src/main.rs`
- Modify: `crates/fixer-server/src/app.rs`
- Delete after callers migrate: `crates/fixer-server/src/jobs/artifacts.rs`
- Delete after callers migrate: `crates/fixer-server/src/jobs/events.rs`
- Delete after callers migrate: `crates/fixer-server/src/jobs/model.rs`
- Delete after callers migrate: `crates/fixer-server/src/jobs/worker.rs`
- Delete after callers migrate: `crates/fixer-server/src/jobs/mod.rs`
- Create: `crates/fixer-server/tests/scrapes.rs`
- Modify: `crates/fixer-server/tests/startup.rs`

**Step 1: Write failing queue tests**

Use a small bounded queue and controlled fixture worker. Cover FIFO execution, one active filesystem writer, backpressure, channel closure, per-run panic containment, and terminal result storage.

**Step 2: Write failing restart tests**

Persist queued and running rows, restart runtime, and assert both are requeued exactly once while terminal rows are not.

**Step 3: Verify failure**

Run: `cargo test -p fixer-server --test scrapes --test startup`

Expected: FAIL before `ScrapeRuntime` exists.

**Step 4: Implement bounded Tokio dispatch**

Use `tokio::sync::mpsc::channel<ScrapeRunId>`. Reserve capacity before inserting API-created work, send only IDs, and run one receiver loop. On startup atomically reset running rows and enqueue all queued IDs. Do not add a homegrown queue, polling scheduler, or Iggy dependency.

**Step 5: Implement complete worker flow**

The worker loads the run, scans local media, calls the SDK automatic/exact scrape workflow, builds output, executes it, records fingerprints, and writes one immutable success/failure artifact. Progress events use transient user-readable phases.

**Step 6: Verify runtime tests**

Run: `cargo test -p fixer-server --test scrapes --test startup`

Expected: PASS.

**Step 7: Commit**

```bash
git add crates/fixer-server/src crates/fixer-server/tests
 git commit -m "feat(server): execute scrape runs through tokio queue"
```

### Task 7: Replace the public job API with scrape APIs

**Files:**

- Create: `crates/fixer-server/src/api/v1/scrapes.rs`
- Modify: `crates/fixer-server/src/api/v1/mod.rs`
- Modify: `crates/fixer-server/src/api/mod.rs`
- Modify: `crates/fixer-server/src/api/error.rs`
- Delete: `crates/fixer-server/src/api/v1/jobs.rs`
- Modify: `crates/fixer-server/tests/api.rs`
- Modify: `crates/fixer-server/tests/auth.rs`
- Create: `crates/fixer-server/tests/scrape_api.rs`
- Replace: `crates/fixer-server/tests/e2e_jobs.rs` with `crates/fixer-server/tests/e2e_scrapes.rs`

**Step 1: Write failing contract tests**

Cover create/list/get/events, automatic and exact selections, TMDB validation, CSRF, idempotency, bounded list limits, retry, correction options, correction creation, and safe path display. Assert `/api/v1/jobs` returns 404.

**Step 2: Verify failure**

Run: `cargo test -p fixer-server --test scrape_api --test api --test auth`

Expected: FAIL because only job routes exist.

**Step 3: Implement versioned scrape DTOs**

Expose public state, transient progress, persisted result/failure, and linked history without leaking filesystem roots or internal worker details. Treat correction candidate identity as untrusted and fetch authoritative metadata by provider target.

**Step 4: Implement correction and retry**

Retry accepts only failed runs and clones selection/input. Correction accepts a candidate identity or exact TMDB ID, finds the prior successful output manifest, creates a linked correction run, and queues it without mutating the earlier run.

**Step 5: Run API and E2E server tests**

Run: `cargo test -p fixer-server --test scrape_api --test e2e_scrapes --test api --test auth`

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-server/src/api crates/fixer-server/tests
git commit -m "feat(server): expose scrape audit APIs"
```

### Task 8: Move folder ingestion from jobs to scrape runs

**Files:**

- Modify: `crates/fixer-server/src/ingestion/model.rs`
- Modify: `crates/fixer-server/src/ingestion/mod.rs`
- Modify: `crates/fixer-server/src/ingestion/watcher.rs`
- Modify: `crates/fixer-server/src/api/v1/ingestion.rs`
- Modify: `crates/fixer-server/src/store/sqlite.rs`
- Modify: `crates/fixer-server/tests/ingestion_automation.rs`
- Modify: `crates/fixer-server/tests/ingestion_rules.rs`
- Modify: `crates/fixer-server/tests/ingestion_watch.rs`
- Modify: `crates/fixer-server/tests/e2e_ingestion.rs`

**Step 1: Rewrite failing ingestion expectations**

Assert discovered stable sources create initial automatic scrape runs, status becomes processing then succeeded/failed, duplicate fingerprints remain deduplicated, and no needs-review status exists.

**Step 2: Verify failure**

Run: `cargo test -p fixer-server --test ingestion_automation --test ingestion_rules --test ingestion_watch --test e2e_ingestion`

Expected: FAIL while ingestion creates jobs and branches on confidence/review.

**Step 3: Implement scrape integration**

Replace job IDs and activity SQL with scrape run IDs/statuses. Rule-origin runs use the same automatic selection and queue as manual runs. Ambiguous media kind becomes a failed run with an actionable error unless the rule fixes the kind; it never enters review.

**Step 4: Run ingestion tests**

Run: `cargo test -p fixer-server --test ingestion_automation --test ingestion_rules --test ingestion_watch --test e2e_ingestion`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-server/src/ingestion crates/fixer-server/src/api/v1/ingestion.rs crates/fixer-server/src/store/sqlite.rs crates/fixer-server/tests
git commit -m "refactor(server): create scrape runs from folder ingestion"
```

### Task 9: Replace the Web job workflow with one-click scrape and audit pages

**Files:**

- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/api.test.ts`
- Modify: `web/src/lib/sse.ts`
- Modify: `web/src/lib/sse.test.ts`
- Modify: `web/src/components/app-shell.tsx`
- Create: `web/src/components/scrape-status.tsx`
- Create: `web/src/routes/scrapes/index.tsx`
- Create: `web/src/routes/scrapes/$scrapeId/index.tsx`
- Create: `web/src/routes/scrapes/scrapes.test.tsx`
- Modify: `web/src/routes/index.tsx`
- Modify: `web/src/routes/folders.tsx`
- Modify: `web/src/routes/folders.test.tsx`
- Modify: `web/src/app.test.tsx`
- Delete: `web/src/components/job-status.tsx`
- Delete: `web/src/components/progress-timeline.tsx`
- Delete: `web/src/routes/jobs/index.tsx`
- Delete: `web/src/routes/jobs/$jobId/index.tsx`
- Delete: `web/src/routes/jobs/$jobId/review.tsx`
- Delete: `web/src/routes/jobs/$jobId/plan.tsx`
- Delete: `web/src/routes/jobs/jobs.test.tsx`
- Regenerate: `web/src/routeTree.gen.ts`

**Step 1: Write failing API client tests**

Assert request/response shapes for scrape creation, exact TMDB selection, list/get/events, retry, correction search, and correction creation. Assert no public client methods reference jobs, reviews, plans, or execution approval.

**Step 2: Write failing page tests**

Cover:

- source selection plus one `开始刮削` click;
- optional `指定作品` with movie/television and TMDB ID;
- `刮削已开始，可以离开此页面` confirmation;
- queued/running/succeeded/failed record rows;
- persisted result audit rendering;
- correction search and direct TMDB correction;
- linked history and legacy-incomplete notice;
- no visible `Job`, confidence, review, plan, or execute-plan text.

**Step 3: Verify failure**

Run: `pnpm --dir web test -- lib/api.test.ts routes/scrapes/scrapes.test.tsx app.test.tsx`

Expected: FAIL because scrape routes and client contracts do not exist.

**Step 4: Implement the API client and SSE mapping**

Add discriminated TypeScript types matching schema version 1. Keep server state in TanStack Query and invalidate the list/detail queries on SSE terminal events.

**Step 5: Implement the scrape page**

Reuse `DirectoryPicker` and existing rule lookup. Hide organization fields when a rule supplies them. Put media kind and TMDB ID under a native accessible disclosure. Submit with one primary action and do not navigate away automatically.

**Step 6: Implement records and audit**

Use `/scrapes` and `/scrapes/$scrapeId`. Render only four public statuses and plain-language transient phases. Show selected identity, metadata, file changes, warnings/failure, retry/correction, and history. Keep controls keyboard accessible and usable at 320, 768, 1024, and 1440 px.

**Step 7: Remove job UI and regenerate routes**

Delete old pages/components, update navigation labels, add an optional client redirect from stale `/jobs` links to `/scrapes`, and regenerate `routeTree.gen.ts` through the existing Vite/TanStack command rather than editing generated output manually.

**Step 8: Run Web gates**

Run: `pnpm --dir web typecheck && pnpm --dir web test && pnpm --dir web build`

Expected: PASS with no console/type errors.

**Step 9: Commit**

```bash
git add web
git commit -m "feat(web): add one-click scrape audit experience"
```

### Task 10: Update documentation and real end-to-end behavior

**Files:**

- Modify: `README.md`
- Modify: `docs/server.md`
- Modify: `docs/security.md`
- Modify: `docs/troubleshooting.md`
- Modify: `docs/configuration.md`
- Modify: `docs/sdk.md`
- Modify: `docs/cli.md`
- Modify: `web/e2e/critical-flow.spec.ts`
- Modify: `scripts/e2e-local.sh` if route or fixture setup changes

**Step 1: Rewrite the browser critical flow**

Make it start a scrape, leave the page, wait from the records page, audit the persisted result, correct it with a different exact TMDB fixture ID, and verify both audit records and safe replacement.

**Step 2: Run E2E and inspect the expected failure**

Run: `pnpm --dir web exec playwright test web/e2e/critical-flow.spec.ts`

Expected: FAIL until route fixtures and assertions use scrape APIs.

**Step 3: Update docs**

Document one-click semantics, exact provider IDs, deterministic first selection, four run states, correction safety, persistence, queue recovery, API routes, SDK scrape examples, removed confidence settings, and legacy migration. Remove instructions for review, plan approval, execute, and Job inspection.

**Step 4: Run targeted documentation and E2E checks**

Run: `bash scripts/check-config-docs.sh && pnpm --dir web exec playwright test web/e2e/critical-flow.spec.ts`

Expected: PASS.

**Step 5: Run the full acceptance gate**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
bash scripts/check-config-docs.sh
```

Expected: every command exits 0.

**Step 6: Mechanically verify removed concepts and new public symbols**

Run:

```bash
rg -n "Job|job|awaiting_confirmation|review_confidence|auto_accept_confidence|Confidence" crates/fixer-server/src web/src fixer.toml.example
rg -n "ScrapeRun|ScrapeSelection|ProviderTarget|/api/v1/scrapes" crates web/src docs
```

Expected: the first command finds no live server/Web domain references except intentional legacy migration text or compatibility redirects; the second finds all required registrations and public contracts.

**Step 7: Commit**

```bash
git add README.md docs web/e2e scripts
git commit -m "docs: document scrape runs and correction workflow"
```

## Final Review

- Review the complete diff for accidental deletion of unrelated auth, root policy, provider, or folder-watcher behavior.
- Confirm migrations operate on both a fresh database and a pre-0008 database.
- Confirm partial execution failures preserve operation-level audit evidence.
- Confirm exact-ID corrections cannot overwrite files changed after the prior run.
- Confirm no browser route or API asks the user to review a candidate, approve a plan, or execute a job.
- Run `git status --short` and ensure only intended changes remain.
