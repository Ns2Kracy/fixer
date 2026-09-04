# Folder Ingestion Rules Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the user-facing workspace/manual-path flow with persistent `src -> dst` ingestion rules that discover existing media, watch recursively, scrape metadata, and organize safe high-confidence matches automatically.

**Architecture:** Add rule and source-observation records to the existing SQLite store, resolve browser directory references through the canonical media-root policy, and run one hybrid ingestion supervisor beside the existing job workers. Rule-created jobs carry immutable destination, placement, template, origin, and automation snapshots; the existing worker lifecycle performs confidence-gated review and execution. The Solid UI adds a Folders route with server-backed directory pickers and removes user-visible workspace language and global placement controls.

**Tech Stack:** Rust 2024, Axum, Tokio, SQLx/SQLite, `notify`, existing Fixer Core/SDK/local providers and writers, Solid 2, TanStack Router/Query, Vitest, Playwright.

---

## Working rules

- Work in a dedicated feature worktree or branch based on commit `52f48e1`.
- Do not stage or modify the user's existing `web/pnpm-lock.yaml` change unless dependency installation itself requires a deliberate lockfile update; inspect its existing diff first and preserve it.
- Follow `@test-driven-development`: every behavior starts with a failing test, then the smallest implementation.
- Use `@axum-web-framework` and `@api-and-interface-design` for new HTTP contracts.
- Use `@frontend-ui-engineering` and browser verification for the Folders and directory-picker flows.
- Use `@verification-before-completion` and `@code-review-and-quality` before the final commit.
- Keep internal `WorkspaceState` names unless renaming them makes the implementation smaller; the requirement is to remove the concept from user-visible copy.

### Task 1: Make match confidence executable policy

**Files:**

- Modify: `crates/fixer-core/src/matching/score.rs`
- Modify: `crates/fixer-core/tests/matching.rs`
- Modify: `crates/fixer-server/src/jobs/artifacts.rs`
- Modify: `web/src/lib/api.ts`

**Step 1: Write failing confidence tests**

Add table-driven tests proving that confidence is normalized to `0.0..=1.0`, exact external IDs are `1.0`, exact title/year evidence clears `0.9`, partial titles remain below `0.9`, and negative year/sequence evidence lowers confidence.

```rust
#[test]
fn confidence_normalizes_available_evidence() {
    let exact = Matcher.score(&exact_query(), &exact_candidate()).unwrap();
    let partial = Matcher.score(&exact_query(), &partial_candidate()).unwrap();

    assert!((exact.confidence() - 1.0).abs() < f32::EPSILON);
    assert!(partial.confidence() < 0.9);
    assert!((0.0..=1.0).contains(&partial.confidence()));
}
```

**Step 2: Run the test and verify RED**

Run: `cargo test -p fixer-core --test matching confidence --locked`

Expected: FAIL because `MatchScore::confidence` does not exist.

**Step 3: Implement normalized confidence**

Add `MatchScore::confidence()`. Return `1.0` for positive exact external-ID evidence. Otherwise divide the earned total by the sum of the maximum positive weight for each evidence kind present (`Title=100`, `Alias=70`, `Year=20`, `Sequence=50`) and clamp to `0.0..=1.0`. Keep the current integer score for deterministic ranking.

Expose `confidence: f32` beside `score` in `CandidateArtifact`; update the Web DTO.

**Step 4: Run targeted tests and typecheck**

Run: `cargo test -p fixer-core --test matching confidence --locked && cargo test -p fixer-server artifacts --locked && pnpm --dir web typecheck`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-core/src/matching/score.rs crates/fixer-core/tests/matching.rs crates/fixer-server/src/jobs/artifacts.rs web/src/lib/api.ts
git commit -m "feat: expose normalized match confidence"
```

### Task 2: Add safe move operations

**Files:**

- Modify: `crates/fixer-core/src/output/plan.rs`
- Modify: `crates/fixer-core/tests/contracts.rs`
- Modify: `crates/fixer-sdk/src/output/executor.rs`
- Modify: `crates/fixer-sdk/tests/output_execution.rs`
- Modify: `crates/fixer-server/src/jobs/artifacts.rs`
- Modify: `crates/fixer-cli/src/json.rs`

**Step 1: Write failing plan and executor tests**

Test serialization/preview of a `Move` operation, successful same-filesystem movement, no overwrite on collision, and source preservation when publication fails.

```rust
#[test]
fn move_publishes_destination_then_removes_source() {
    let source_root = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let source = source_root.path().join("movie.mkv");
    std::fs::write(&source, b"media").unwrap();

    let mut plan = OutputPlan::new(destination.path());
    plan.push(OutputOperation::move_file(&source, "Movie/movie.mkv").unwrap());
    plan.execute(ExecutionPolicy::default()).unwrap();

    assert!(!source.exists());
    assert_eq!(std::fs::read(destination.path().join("Movie/movie.mkv")).unwrap(), b"media");
}
```

**Step 2: Verify RED**

Run: `cargo test -p fixer-sdk --test output_execution move_ --locked`

Expected: FAIL because `OutputOperation::Move` and `move_file` do not exist.

**Step 3: Implement the operation**

Add:

```rust
Move { source: PathBuf, target: PathBuf }
```

Include it in `source()`, `target()`, JSON rendering, server operation previews, filesystem-policy validation, and every exhaustive match. Executor behavior:

1. validate source and target;
2. try `std::fs::rename` into the final target when safe;
3. on `ErrorKind::CrossesDevices`, copy into the existing unique temporary-file mechanism;
4. atomically publish the temporary file;
5. remove the source only after publication succeeds;
6. never overwrite under the default policy and never silently fall back for another operation kind.

**Step 4: Run affected suites**

Run: `cargo test -p fixer-core --test contracts --locked && cargo test -p fixer-sdk --test output_execution --locked && cargo test -p fixer-server artifacts --locked`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-core/src/output/plan.rs crates/fixer-core/tests/contracts.rs crates/fixer-sdk/src/output/executor.rs crates/fixer-sdk/tests/output_execution.rs crates/fixer-server/src/jobs/artifacts.rs crates/fixer-cli/src/json.rs
git commit -m "feat: add safe move output operations"
```

### Task 3: Remove global placement ownership

**Files:**

- Modify: `crates/fixer-runtime/src/config.rs`
- Modify: `crates/fixer-runtime/tests/config.rs`
- Modify: `crates/fixer-cli/src/args.rs`
- Modify: `crates/fixer-cli/src/config.rs`
- Modify: `crates/fixer-cli/src/commands/scrape.rs`
- Modify: relevant files under `crates/fixer-cli/tests/`
- Modify: `crates/fixer-server/src/workspace.rs`
- Modify: `crates/fixer-server/tests/workspace.rs`
- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/api.test.ts`
- Modify: `web/src/routes/settings.tsx`
- Modify: `web/src/routes/workspace.test.tsx`
- Modify: `fixer.toml.example`
- Modify: `docs/configuration.md`
- Modify: `docs/cli.md`

**Step 1: Write failing ownership tests**

Change configuration tests so global settings neither deserialize, serialize, nor return `placement`. Change CLI tests so `plan`/`scrape` require an explicit `--placement` value. Change Web settings tests so the global Placement field is absent.

**Step 2: Verify RED**

Run: `cargo test -p fixer-runtime config --locked && cargo test -p fixer-cli --test cli --locked && pnpm --dir web test -- src/routes/workspace.test.tsx`

Expected: FAIL while global placement still exists.

**Step 3: Implement the ownership change**

Keep the placement enum as a reusable command/rule value, including `move`, but remove `FixerConfig::placement`, its environment/config default, settings API field, settings form, and validation summary entry. Make CLI placement explicit per invocation rather than falling back to shared configuration. Do not add a rule-level default.

Retain migration-friendly diagnostics for obsolete `placement` config input if the loader already has a compatibility mechanism; otherwise update committed examples and tests together because the project is pre-stable.

**Step 4: Run focused checks**

Run: `scripts/check-config-docs.sh && cargo test -p fixer-runtime --locked && cargo test -p fixer-cli --locked && cargo test -p fixer-server --test workspace --locked && pnpm --dir web test -- src/lib/api.test.ts src/routes/workspace.test.tsx`

Expected: PASS and no global Placement control.

**Step 5: Commit**

```bash
git add crates/fixer-runtime crates/fixer-cli crates/fixer-server/src/workspace.rs crates/fixer-server/tests/workspace.rs web/src/lib/api.ts web/src/lib/api.test.ts web/src/routes/settings.tsx web/src/routes/workspace.test.tsx fixer.toml.example docs/configuration.md docs/cli.md
git commit -m "refactor: make media placement operation scoped"
```

### Task 4: Introduce stable safe directory references

**Files:**

- Modify: `crates/fixer-server/src/workspace.rs`
- Modify: `crates/fixer-server/src/fs_policy.rs`
- Modify: `crates/fixer-server/src/api/v1/workspace.rs`
- Modify: `crates/fixer-server/tests/workspace.rs`
- Modify: `crates/fixer-server/tests/fs_policy.rs`

**Step 1: Write failing directory-reference tests**

Cover stable root IDs across root reordering, relative directory resolution, file rejection, traversal rejection, symlink escape rejection, and overlap rejection.

```rust
#[test]
fn directory_refs_remain_stable_when_roots_reorder() {
    let first = WorkspaceState::new([root_a.path(), root_b.path()]).unwrap();
    let second = WorkspaceState::new([root_b.path(), root_a.path()]).unwrap();
    assert_eq!(first.roots_for_test(), second.roots_for_test());
}
```

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test workspace directory_ref --locked`

Expected: FAIL because roots currently use order-dependent IDs such as `root-0`.

**Step 3: Implement directory references**

Add serializable input/output contracts:

```rust
pub(crate) struct DirectoryRef {
    pub root_id: String,
    pub path: String,
}

pub(crate) struct ResolvedDirectory {
    pub root_id: String,
    pub relative_path: String,
    pub canonical_path: PathBuf,
}
```

Derive each opaque root ID from a stable digest of its canonical path. Add `WorkspaceState::resolve_directory` and `WorkspaceState::display_directory`. Reuse `resolve_relative` and `FsPolicy`; never include canonical paths in API errors or response DTOs. Add a helper that rejects equal/ancestor source-destination pairs.

**Step 4: Run focused tests**

Run: `cargo test -p fixer-server --test fs_policy --locked && cargo test -p fixer-server --test workspace --locked`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-server/src/workspace.rs crates/fixer-server/src/fs_policy.rs crates/fixer-server/src/api/v1/workspace.rs crates/fixer-server/tests/workspace.rs crates/fixer-server/tests/fs_policy.rs
git commit -m "feat: add stable safe directory references"
```

### Task 5: Persist ingestion rules and observed sources

**Files:**

- Create: `crates/fixer-server/migrations/0006_ingestion_rules.sql`
- Create: `crates/fixer-server/src/ingestion/mod.rs`
- Create: `crates/fixer-server/src/ingestion/model.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Modify: `crates/fixer-server/src/store/mod.rs`
- Modify: `crates/fixer-server/src/store/sqlite.rs`
- Modify: `crates/fixer-server/tests/store.rs`

**Step 1: Write failing persistence tests**

Test create/list/get/update/delete, required placement, fixed/auto media mode, optional template override, enable/disable, duplicate source fingerprint reservation, job association, and restart persistence.

```rust
#[tokio::test]
async fn ingestion_rules_and_observations_survive_reopen() {
    let store = SqliteJobStore::open(&database).await.unwrap();
    let rule = store.create_ingestion_rule(valid_rule()).await.unwrap();
    assert!(store.reserve_source(rule.id(), fingerprint()).await.unwrap().is_reserved());
    drop(store);

    let reopened = SqliteJobStore::open(&database).await.unwrap();
    assert_eq!(reopened.get_ingestion_rule(rule.id()).await.unwrap().unwrap(), rule);
    assert!(!reopened.reserve_source(rule.id(), fingerprint()).await.unwrap().is_reserved());
}
```

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test store ingestion --locked`

Expected: FAIL because the model and store methods do not exist.

**Step 3: Add schema and model**

Create `ingestion_rules` with bounded name, source/destination root IDs and relative paths, media mode, required placement, optional path template, enabled flag, last error, and timestamps. Create `ingestion_sources` with rule FK, relative source path, size, modified time, status, optional job FK, and a unique fingerprint constraint.

Use typed enums:

```rust
pub enum MediaKindMode { Auto, Fixed(JobMediaKind) }
pub enum RulePlacement { Move, Copy, Hardlink, Symlink, Reflink }
pub enum RuleStatus { Watching, Processing, NeedsReview, Paused, Error }
```

Do not include `InPlace` and do not implement `Default` for `RulePlacement`.

**Step 4: Implement store methods and verify**

Add bounded CRUD and reservation methods directly to `SqliteJobStore`, following current SQLx transaction, timestamp, decode, and `StoreError::CorruptRecord` patterns.

Run: `cargo test -p fixer-server --test store ingestion --locked`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-server/migrations/0006_ingestion_rules.sql crates/fixer-server/src/ingestion crates/fixer-server/src/lib.rs crates/fixer-server/src/store crates/fixer-server/tests/store.rs
git commit -m "feat: persist folder ingestion rules"
```

### Task 6: Expose authenticated rule CRUD

**Files:**

- Create: `crates/fixer-server/src/api/v1/ingestion.rs`
- Modify: `crates/fixer-server/src/api/v1/mod.rs`
- Modify: `crates/fixer-server/src/app.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Create: `crates/fixer-server/tests/ingestion_rules.rs`
- Modify: `crates/fixer-server/tests/auth.rs`

**Step 1: Write failing API tests**

Cover:

- `GET /api/v1/ingestion-rules`;
- `POST /api/v1/ingestion-rules`;
- `PUT /api/v1/ingestion-rules/{id}`;
- `DELETE /api/v1/ingestion-rules/{id}`;
- `POST /api/v1/ingestion-rules/{id}/scan`;
- auth and CSRF enforcement;
- no absolute paths in success or error bodies;
- `422` for missing placement, invalid template, source/destination overlap, file selection, and removed roots.

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test ingestion_rules --locked`

Expected: FAIL with route not found.

**Step 3: Implement `IngestionRuntime` and routes**

Construct `IngestionRuntime` from `SqliteJobStore`, `JobRuntime`, and `WorkspaceState`. Route handlers accept `DirectoryRef` values, resolve and validate them, persist only validated rule data, and notify a reload handle after successful mutations. The first version of `/scan` only records a rescan request; Task 9 consumes it.

Merge the router only into the authenticated production API. Keep pure job/workspace test routers available with their existing signatures where practical.

**Step 4: Verify focused server tests**

Run: `cargo test -p fixer-server --test ingestion_rules --locked && cargo test -p fixer-server --test auth --locked && cargo test -p fixer-server --test jobs --locked`

Expected: PASS.

**Step 5: Commit**

```bash
git add crates/fixer-server/src/api/v1/ingestion.rs crates/fixer-server/src/api/v1/mod.rs crates/fixer-server/src/app.rs crates/fixer-server/src/lib.rs crates/fixer-server/tests/ingestion_rules.rs crates/fixer-server/tests/auth.rs
git commit -m "feat: add ingestion rule API"
```

### Task 7: Make jobs carry destination and organization snapshots

**Files:**

- Create: `crates/fixer-writer-local/src/organize.rs`
- Modify: `crates/fixer-writer-local/src/lib.rs`
- Create: `crates/fixer-writer-local/tests/organize.rs`
- Modify: `crates/fixer-cli/src/commands/scrape.rs`
- Modify: `crates/fixer-server/src/jobs/model.rs`
- Modify: `crates/fixer-server/src/jobs/worker.rs`
- Modify: `crates/fixer-server/src/jobs/artifacts.rs`
- Modify: `crates/fixer-server/src/api/v1/jobs.rs`
- Modify: `crates/fixer-server/tests/job_model.rs`
- Modify: `crates/fixer-server/tests/jobs.rs`

**Step 1: Write failing organizer and compatibility tests**

Prove built-in destination layouts for every media kind, custom relative template validation, source-extension preservation, each organization method's operation type, and deserialization of old jobs without an organization snapshot.

```rust
#[test]
fn movie_preset_places_media_beneath_destination() {
    let request = organization_request("Arrival.mkv", resolved_movie(), RulePlacement::Hardlink);
    let plan = organize(request).unwrap();
    assert_eq!(plan.output_root, PathBuf::from("/library"));
    assert!(matches!(plan.operations()[0], OutputOperation::Hardlink { .. }));
    assert_eq!(plan.operations()[0].target().unwrap(), Path::new("Arrival (2016)/Arrival (2016).mkv"));
}
```

**Step 2: Verify RED**

Run: `cargo test -p fixer-writer-local --test organize --locked && cargo test -p fixer-server --test job_model organization --locked`

Expected: FAIL because shared organization planning and job snapshots do not exist.

**Step 3: Extract shared destination planning**

Move reusable folder-name, media output-root, placement-mode, and manifest reconciliation logic out of CLI `scrape.rs` into `fixer-writer-local::organize`. Provide built-in presets for movie, television, anime, music, and book plus optional validated path-template overrides. Keep targets relative to `dst` and preserve media extensions.

**Step 4: Extend job input compatibly**

Add an optional `organization` field to persisted `JobInputDto`:

```rust
pub struct JobOrganizationDto {
    pub destination_path: String,
    pub placement: RulePlacement,
    pub path_template: Option<String>,
    pub origin_rule_id: Option<i64>,
    pub auto_execute: bool,
}
```

Use `#[serde(default, skip_serializing_if = "Option::is_none")]` so existing `input_json` rows still decode. Rule-created jobs always provide it. The worker scans `input_path` but plans against `destination_path`, adding the selected source-media operation and metadata operations to one bounded plan.

**Step 5: Run focused suites**

Run: `cargo test -p fixer-writer-local --locked && cargo test -p fixer-cli --locked && cargo test -p fixer-server --test job_model --locked && cargo test -p fixer-server --test jobs --locked`

Expected: PASS.

**Step 6: Commit**

```bash
git add crates/fixer-writer-local crates/fixer-cli/src/commands/scrape.rs crates/fixer-server/src/jobs crates/fixer-server/src/api/v1/jobs.rs crates/fixer-server/tests/job_model.rs crates/fixer-server/tests/jobs.rs
git commit -m "feat: plan jobs from source to destination"
```

### Task 8: Discover one logical item per directory entry

**Files:**

- Create: `crates/fixer-server/src/ingestion/discovery.rs`
- Modify: `crates/fixer-server/src/ingestion/mod.rs`
- Create: `crates/fixer-server/tests/ingestion_discovery.rs`
- Use fixtures: `tests/fixtures/library/`

**Step 1: Write failing discovery tests**

Cover a source directory containing multiple movies, nested television/anime roots, music releases, and books. Fixed mode must emit only that type. Auto mode must emit one unique type or a bounded `NeedsReview` record when multiple scanners claim the same root.

```rust
#[test]
fn fixed_movie_directory_emits_one_job_per_movie() {
    let items = discover(&source, MediaKindMode::Fixed(JobMediaKind::Movie)).unwrap();
    assert_eq!(items.iter().map(DiscoveredItem::source_root).collect::<Vec<_>>(), [movie_a, movie_b]);
}
```

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test ingestion_discovery --locked`

Expected: FAIL because directory discovery does not exist and current jobs reject multi-document roots as ambiguous.

**Step 3: Implement fixed-kind discovery**

Call the existing local scanners once per rule source and zip their documents with scanner roots. Canonicalize and deduplicate roots, then emit one `DiscoveredItem` per logical media object. Event-path reconciliation selects the nearest discovered root containing the changed path.

**Step 4: Implement bounded auto detection**

Run applicable scanners and group claims by canonical source root. Emit a typed item for one claim. Emit `NeedsReview` for multiple media-kind claims instead of guessing. Unsupported files are recorded as ignored so reconciliation does not repeatedly enqueue them.

**Step 5: Verify and commit**

Run: `cargo test -p fixer-server --test ingestion_discovery --locked`

Expected: PASS.

```bash
git add crates/fixer-server/src/ingestion crates/fixer-server/tests/ingestion_discovery.rs
git commit -m "feat: discover media items from folders"
```

### Task 9: Run the hybrid ingestion supervisor

**Files:**

- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `crates/fixer-server/Cargo.toml`
- Create: `crates/fixer-server/src/ingestion/watcher.rs`
- Modify: `crates/fixer-server/src/ingestion/mod.rs`
- Modify: `crates/fixer-server/src/lib.rs`
- Create: `crates/fixer-server/tests/ingestion_watch.rs`

**Step 1: Write failing reconciliation tests**

Test initial recursive discovery, changed-file rediscovery, unchanged fingerprint deduplication, modified fingerprint reprocessing, destination/temp-file exclusion, disabled rules, missing directory recovery, and explicit rescan.

Use short injected intervals only in tests; production defaults remain fixed and minimal.

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test ingestion_watch --locked`

Expected: FAIL because no supervisor exists.

**Step 3: Add the watcher dependency**

Add `notify` to workspace/server dependencies. Use Tokio channels and timers already present; do not add a separate debounce framework.

**Step 4: Implement one supervisor**

`IngestionSupervisor` owns one `notify::RecommendedWatcher`, a reload notification, a rescan channel, and a periodic reconciliation interval. On startup and rule reload it recursively reconciles all enabled rules and registers their sources with `RecursiveMode::Recursive`. Events only mark affected rules dirty; reconciliation performs canonical validation, stability checks, discovery, and source reservation.

A candidate is stable only after two observations have the same size and modification time across the debounce interval. Store reservation happens before `JobRuntime::create`; associate the job afterward. On creation failure, mark the observation retryable.

**Step 5: Wire lifecycle and verify**

Start the supervisor in `serve_inner` after the store, workspace state, and job runtime are ready. Shut it down through the existing cancellation path. Watcher failure records rule error state but does not terminate Axum or workers.

Run: `cargo test -p fixer-server --test ingestion_watch --locked && cargo test -p fixer-server --test startup --locked`

Expected: PASS.

**Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/fixer-server/Cargo.toml crates/fixer-server/src/ingestion crates/fixer-server/src/lib.rs crates/fixer-server/tests/ingestion_watch.rs
git commit -m "feat: watch and reconcile ingestion folders"
```

### Task 10: Auto-review and execute only safe matches

**Files:**

- Modify: `crates/fixer-server/src/jobs/mod.rs`
- Modify: `crates/fixer-server/src/jobs/worker.rs`
- Modify: `crates/fixer-server/src/jobs/model.rs`
- Modify: `crates/fixer-server/src/ingestion/mod.rs`
- Modify: `crates/fixer-server/tests/jobs.rs`
- Create: `crates/fixer-server/tests/ingestion_automation.rs`

**Step 1: Write failing automation tests**

Cover:

- unique top candidate at threshold, no conflicts, valid plan: review and execute automatically;
- confidence exactly below threshold: `awaiting_confirmation`;
- tied top candidates: `awaiting_confirmation`;
- merge conflicts: `awaiting_confirmation`;
- destination collision or invalid plan: no write and inspectable failure/review;
- manual job with the same score: no automatic execution;
- disabled/deleted rule after queueing: immutable job snapshot still controls behavior.

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test ingestion_automation --locked`

Expected: FAIL because all jobs currently stop for explicit review.

**Step 3: Implement one automation gate**

Add a pure decision function and test it directly:

```rust
fn auto_decision(input: &JobInputDto, review: &ReviewArtifacts, conflicts: u64, threshold: f32) -> AutoDecision
```

Read the threshold from `auto_accept_confidence`. Return `Execute { candidate_index }` only when the job snapshot has `auto_execute=true`, the top candidate is unique, its normalized confidence meets that threshold, conflicts are zero, and the generated plan passes `FsPolicy::validate_plan`. Otherwise return `NeedsReview` with a bounded reason.

**Step 4: Integrate with worker transitions**

Reuse the same review-decision persistence, plan persistence, execution reservation, idempotency, and `OutputPlanExt` execution paths used by HTTP review/execute. Do not create a second filesystem executor. Publish normal job events for every transition.

**Step 5: Verify and commit**

Run: `cargo test -p fixer-server --test ingestion_automation --locked && cargo test -p fixer-server --test jobs --locked && cargo test -p fixer-server --test e2e_jobs --locked`

Expected: PASS.

```bash
git add crates/fixer-server/src/jobs crates/fixer-server/src/ingestion crates/fixer-server/tests/jobs.rs crates/fixer-server/tests/ingestion_automation.rs
git commit -m "feat: auto-organize high-confidence ingestion jobs"
```

### Task 11: Add Web API contracts and an accessible directory picker

**Files:**

- Modify: `web/src/lib/api.ts`
- Modify: `web/src/lib/api.test.ts`
- Create: `web/src/components/directory-picker.tsx`
- Create: `web/src/components/directory-picker.test.tsx`

**Step 1: Write failing API-client tests**

Add typed requests/responses and exact URL/method/body assertions for rule CRUD and rescan. Model `DirectoryRef`, media mode, placement, template override, rule status, and last error. Ensure placement is a required union without an empty/default member.

**Step 2: Verify RED**

Run: `pnpm --dir web test -- src/lib/api.test.ts`

Expected: FAIL because methods are absent.

**Step 3: Implement API methods**

Add `listIngestionRules`, `createIngestionRule`, `updateIngestionRule`, `deleteIngestionRule`, and `rescanIngestionRule` using the existing `#request` and CSRF behavior.

**Step 4: Write and implement picker tests**

Test root selection, breadcrumb navigation, directory-only selection, Back/Escape behavior, focus restoration, loading/error/empty states, and that no absolute-path text input exists.

Reuse `api.libraryRoots()` and `api.listLibrary()`; render entries as semantic buttons inside a labelled dialog or inline panel. Do not add a browser file-input workaround.

Run: `pnpm --dir web test -- src/components/directory-picker.test.tsx src/lib/api.test.ts`

Expected: PASS.

**Step 5: Commit**

```bash
git add web/src/lib/api.ts web/src/lib/api.test.ts web/src/components/directory-picker.tsx web/src/components/directory-picker.test.tsx
git commit -m "feat: add safe directory picker contracts"
```

### Task 12: Build the Folders rule-management UI

**Files:**

- Create: `web/src/routes/folders.tsx`
- Create: `web/src/routes/folders.test.tsx`
- Modify generated: `web/src/routeTree.gen.ts`
- Modify: `web/src/components/app-shell.tsx`
- Modify: `web/src/components/template-preview.tsx` only if needed for embedded preview

**Step 1: Write failing route tests**

Cover list states, create/edit/delete, enable/disable, rescan, fixed versus auto media mode, required placement, built-in template preset, custom override preview, source/destination pickers, overlap API errors, and status labels.

```tsx
expect(screen.queryByLabelText(/source path/i)).not.toBeInTheDocument();
await user.click(screen.getByRole("button", { name: "Choose source" }));
await user.click(screen.getByRole("button", { name: "Downloads" }));
expect(screen.getByRole("button", { name: "Save rule" })).toBeDisabled();
await user.selectOptions(screen.getByLabelText("Organization method"), "hardlink");
expect(screen.getByRole("button", { name: "Save rule" })).toBeEnabled();
```

**Step 2: Verify RED**

Run: `pnpm --dir web test -- src/routes/folders.test.tsx`

Expected: FAIL because the route does not exist.

**Step 3: Implement the route**

Use a compact operational layout rather than explanatory cards:

- heading and `Add folder rule` action;
- rule rows showing `src -> dst`, media mode, placement, status, and last run;
- direct Pause/Resume, Scan now, Edit, and Delete actions;
- one focused editor with both directory pickers;
- a blank organization-method option so the user must choose;
- built-in template by detected/fixed kind and optional override.

Only show concise labels `Watching`, `Processing`, `Needs review`, `Paused`, and `Error`. Link review counts to Jobs.

**Step 4: Generate routes and verify**

Run: `pnpm --dir web routes:generate && pnpm --dir web test -- src/routes/folders.test.tsx && pnpm --dir web typecheck`

Expected: PASS and `/folders` appears in `routeTree.gen.ts`.

**Step 5: Commit**

```bash
git add web/src/routes/folders.tsx web/src/routes/folders.test.tsx web/src/routeTree.gen.ts web/src/components/app-shell.tsx web/src/components/template-preview.tsx
git commit -m "feat: add folder ingestion management UI"
```

### Task 13: Remove Workspace copy and manual path entry

**Files:**

- Modify: `web/src/routes/index.tsx`
- Modify: `web/src/routes/jobs/index.tsx`
- Modify: `web/src/routes/login.tsx`
- Modify: `web/src/routes/settings.tsx`
- Modify: `web/src/routes/__root.tsx`
- Modify: `web/src/components/app-shell.tsx`
- Modify: `web/src/components/ui/page-header.tsx`
- Modify: `web/src/app.test.tsx`
- Modify: `web/src/routes/login.test.tsx`
- Modify: `web/src/routes/jobs/jobs.test.tsx`
- Modify: `web/src/routes/workspace.test.tsx`
- Modify: `web/e2e/theme.spec.ts`
- Modify: `web/index.html`
- Modify: `web/package.json`

**Step 1: Write failing user-language tests**

Assert that authenticated pages contain no case-insensitive user-visible `workspace`, the home heading is `Overview`, navigation uses `Overview` and `Folders`, settings is `Settings`, and Jobs contains no editable filesystem path field.

**Step 2: Verify RED**

Run: `pnpm --dir web test -- src/app.test.tsx src/routes/login.test.tsx src/routes/jobs/jobs.test.tsx src/routes/workspace.test.tsx`

Expected: FAIL on current copy and `Media path` input.

**Step 3: Simplify the UI**

Replace `Workspace` labels with `Overview`, `app content`, or direct product language. Remove hero prose, implementation-policy descriptions, opaque-root explanations, and redundant section eyebrows where labels already explain the action. Keep accessibility descriptions only when they help operate a control.

Replace the Jobs `Media path` input with the shared source and destination directory pickers. Require media mode and organization method. If one-off jobs remain supported, submit directory references through the resolved job API; do not reintroduce absolute path entry.

**Step 4: Verify Web behavior**

Run: `pnpm --dir web format && pnpm --dir web lint && pnpm --dir web typecheck && pnpm --dir web test && pnpm --dir web build`

Expected: PASS; no user-visible workspace language; no manual path input.

**Step 5: Commit**

```bash
git add web/src web/e2e/theme.spec.ts web/index.html web/package.json
git commit -m "refactor: simplify Fixer user flows"
```

### Task 14: Prove the complete ingestion flow and update operations docs

**Files:**

- Modify: `crates/fixer-server/tests/e2e_jobs.rs`
- Create: `crates/fixer-server/tests/e2e_ingestion.rs`
- Modify: `web/e2e/critical-flow.spec.ts`
- Modify: `README.md`
- Modify: `docs/server.md`
- Modify: `docs/configuration.md`
- Modify: `docs/troubleshooting.md`
- Modify: `fixer.toml.example`

**Step 1: Add failing end-to-end tests**

Backend acceptance flow:

1. create temporary configured source and destination roots;
2. put two fixture media items in source;
3. create an enabled hardlink or copy rule through the authenticated API;
4. start/reload the supervisor;
5. verify initial recursive discovery creates separate jobs;
6. verify a high-confidence item reaches `completed` and exists under destination;
7. verify an ambiguous item remains reviewable;
8. create another source item and verify the live watcher discovers it;
9. restart against the same SQLite database and verify no duplicate jobs.

Browser acceptance flow creates a rule entirely through directory pickers and verifies required placement and concise status labels.

**Step 2: Verify RED**

Run: `cargo test -p fixer-server --test e2e_ingestion --locked && pnpm --dir web test:e2e -- critical-flow.spec.ts`

Expected: FAIL until all integration seams are complete.

**Step 3: Finish integration and documentation**

Document configured-root administration, rule behavior, explicit placement semantics, auto-accept safety gate, watcher reconciliation, restart behavior, and troubleshooting for unavailable roots/link capabilities. Remove documentation that advertises the browser as a workspace or tells users to type paths.

**Step 4: Run complete verification**

Run once after all code changes:

```bash
scripts/check-config-docs.sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --workspace --doc --locked
pnpm --dir web format:check
pnpm --dir web lint
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
pnpm --dir web test:e2e
```

Expected: every command exits zero. Inspect browser console and network errors, keyboard-navigate both directory pickers, and check 320, 768, 1024, and 1440 pixel widths.

**Step 5: Mechanically verify acceptance criteria**

Run searches that must return no user-facing/global configuration hits while allowing internal type names:

```bash
rg -n 'Workspace|workspace' web/src web/index.html web/package.json
rg -n 'FormField label="Media path"|placeholder="/media/' web/src
rg -n 'placement\s*=' fixer.toml.example docs/configuration.md
```

Inspect each remaining match; tests may mention forbidden words only to assert absence. Confirm API symbols, `/folders` route generation, migration `0006`, `notify` registration, and supervisor startup mechanically.

**Step 6: Review and commit**

Run `@code-review-and-quality` against the full branch diff, fix findings, rerun only affected focused checks, then rerun the final gate if code changed across subsystems.

```bash
git add crates/fixer-server/tests/e2e_jobs.rs crates/fixer-server/tests/e2e_ingestion.rs web/e2e/critical-flow.spec.ts README.md docs/server.md docs/configuration.md docs/troubleshooting.md fixer.toml.example
git commit -m "test: verify automatic folder ingestion"
```
