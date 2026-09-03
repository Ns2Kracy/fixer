# Strict Rust and Oxc Quality Tooling Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Commit reproducible, strict, and passing Rust and Solid frontend formatting/linting policies.

**Architecture:** Rust policy lives at the workspace root: rustfmt reads `rustfmt.toml`, Clippy reads behavioral settings from `clippy.toml`, Cargo owns lint levels, and every crate inherits them. Frontend policy lives inside `web`: pinned Oxc packages provide formatter/linter binaries, committed JSON configs define policy, and package scripts expose stable local/CI commands.

**Tech Stack:** Rust 1.85+ / edition 2024, rustfmt 1.9, Clippy, Cargo workspace lints, Solid 2, TypeScript 7, pnpm 11, Oxlint 1.81.0, Oxfmt 0.66.0, oxlint-tsgolint 7.0.2001.

---

### Task 1: Commit the Rust formatter policy

**Files:**

- Create: `rustfmt.toml`
- Format: `crates/**/*.rs`

**Step 1: Create the stable rustfmt configuration**

```toml
edition = "2024"
style_edition = "2024"
max_width = 100
newline_style = "Unix"
use_field_init_shorthand = true
use_try_shorthand = true
```

**Step 2: Verify rustfmt accepts every option**

Run: `cargo fmt --all -- --check`

Expected: either PASS or a diff caused only by the newly explicit policy; no unknown/unstable-option warning.

**Step 3: Apply the declared policy when needed**

Run: `cargo fmt --all`

Review: `git diff --stat` and `git diff --check`. Confirm every changed Rust file is formatting-only.

**Step 4: Verify the formatter gate**

Run: `cargo fmt --all -- --check`

Expected: exit 0.

**Step 5: Commit**

```bash
git add rustfmt.toml crates
git commit -m "style: define the Rust formatting policy"
```

### Task 2: Enforce strict workspace Clippy lints

**Files:**

- Create: `clippy.toml`
- Modify: `Cargo.toml`
- Modify: `crates/fixer-cli/Cargo.toml`
- Modify: `crates/fixer-core/Cargo.toml`
- Modify: `crates/fixer-http/Cargo.toml`
- Modify: `crates/fixer-provider-anilist/Cargo.toml`
- Modify: `crates/fixer-provider-bangumi/Cargo.toml`
- Modify: `crates/fixer-provider-local/Cargo.toml`
- Modify: `crates/fixer-provider-musicbrainz/Cargo.toml`
- Modify: `crates/fixer-provider-openlibrary/Cargo.toml`
- Modify: `crates/fixer-provider-tmdb/Cargo.toml`
- Modify: `crates/fixer-runtime/Cargo.toml`
- Modify: `crates/fixer-sdk/Cargo.toml`
- Modify: `crates/fixer-server/Cargo.toml`
- Modify: `crates/fixer-writer-local/Cargo.toml`
- Modify as required by actionable Clippy findings: `crates/**/*.rs`

**Step 1: Create Clippy behavioral configuration**

```toml
msrv = "1.85"
avoid-breaking-exported-api = true
```

**Step 2: Define workspace lint groups**

Append to root `Cargo.toml`:

```toml
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "deny", priority = -1 }
nursery = { level = "deny", priority = -1 }
cargo = { level = "deny", priority = -1 }
```

Do not enable the complete restriction group.

**Step 3: Make every crate inherit workspace lints**

Add to each member manifest:

```toml
[lints]
workspace = true
```

Mechanically verify that the number of member manifests equals the number containing `workspace = true` under `[lints]`.

**Step 4: Run the strict gate and classify failures**

Run:

```bash
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
```

Expected initially: FAIL on newly enabled pedantic/nursery/cargo lints. Group findings by lint name and count before editing.

**Step 5: Fix actionable findings in small batches**

Prefer code changes for correctness, needless allocation, confusing control flow, API ergonomics, and performance findings. After each batch, rerun the exact Clippy command. Do not use bulk `#[allow]` attributes.

For a rule that is broadly subjective or inapplicable, add a named workspace exception with an adjacent comment. Use an individual local `#[allow(clippy::lint_name, reason = "...")]` only for a narrow false positive.

**Step 6: Verify Rust behavior**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --workspace --doc --locked
```

Expected: all exit 0.

**Step 7: Commit**

```bash
git add Cargo.toml clippy.toml crates
 git commit -m "chore: enforce strict workspace Clippy lints"
```

### Task 3: Initialize Oxfmt and Oxlint for the Solid frontend

**Files:**

- Create: `web/.oxfmtrc.json`
- Create: `web/.oxlintrc.json`
- Modify: `web/package.json`
- Modify: `web/pnpm-lock.yaml`
- Format/fix as required: `web/src/**/*.{ts,tsx,css}`, `web/*.ts`, `web/*.json`

**Step 1: Install pinned Oxc tooling**

Run:

```bash
pnpm --dir web add --save-dev --save-exact \
  oxlint@1.81.0 \
  oxfmt@0.66.0 \
  oxlint-tsgolint@7.0.2001
```

Expected: `package.json` and `pnpm-lock.yaml` change; no runtime dependency changes.

**Step 2: Inspect installed schemas and CLI initialization support**

Run:

```bash
pnpm --dir web exec oxlint --help
pnpm --dir web exec oxfmt --help
```

Use each tool's init command if exposed. Otherwise create the config against the installed `configuration_schema.json`.

**Step 3: Configure Oxfmt**

Create `web/.oxfmtrc.json` with the installed schema and explicit Prettier-compatible policy:

```json
{
  "$schema": "./node_modules/oxfmt/configuration_schema.json",
  "printWidth": 80,
  "tabWidth": 2,
  "useTabs": false,
  "semi": true,
  "singleQuote": false,
  "trailingComma": "all",
  "endOfLine": "lf",
  "ignorePatterns": ["dist", "node_modules", "src/routeTree.gen.ts"]
}
```

Adjust property spelling only when the installed schema requires it. Do not enable import sorting unless its installed configuration guarantees preservation of side-effect import order.

**Step 4: Configure Oxlint**

Create `web/.oxlintrc.json` against `./node_modules/oxlint/configuration_schema.json`:

- enable `options.typeAware` but not experimental `typeCheck`;
- enable correctness, suspicious, pedantic, and performance categories as errors;
- enable built-in `typescript`, `oxc`, `unicorn`, `import`, `promise`, `jsx-a11y`, and `vitest` plugins;
- do not enable `react` or `react-perf`;
- ignore `dist`, `node_modules`, and `src/routeTree.gen.ts`;
- scope Vitest globals/rules to `**/*.test.{ts,tsx}` through overrides.

**Step 5: Add stable package scripts**

Add to `web/package.json`:

```json
"format": "oxfmt --write .",
"format:check": "oxfmt --check .",
"lint": "oxlint ."
```

**Step 6: Establish formatter output**

Run `pnpm --dir web format:check` and confirm it detects any pre-existing drift. Then run `pnpm --dir web format`, review the diff for semantic changes, and rerun `pnpm --dir web format:check` expecting exit 0.

**Step 7: Resolve lint findings**

Run `pnpm --dir web lint`. Fix correctness, accessibility, promise, import, and type-aware findings. Add only named config exceptions for Solid-specific incompatibilities or documented test patterns. Rerun until exit 0.

**Step 8: Verify frontend behavior**

Run:

```bash
pnpm --dir web format:check
pnpm --dir web lint
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

Expected: all exit 0; Vitest reports 57 passing tests unless new focused tests are added.

**Step 9: Commit**

```bash
git add web
git commit -m "chore(web): add strict Oxc quality tooling"
```

### Task 4: Publish the new quality gates

**Files:**

- Modify: `README.md`
- Modify: `docs/development.md`

**Step 1: Add frontend formatter and linter commands**

Add `pnpm --dir web format:check` and `pnpm --dir web lint` before typecheck/tests in the documented local and release-quality command blocks. Explain that Oxfmt and Oxlint configurations are rooted in `web` and installed through the frozen lockfile.

Do not add general CI behavior to `.github/workflows/publish-container.yaml`; that workflow is release-specific, not the repository's general verification pipeline.

**Step 2: Verify documentation commands are exact**

Run:

```bash
rg -n 'format:check|pnpm --dir web lint' README.md docs/development.md
```

Expected: both active quality-gate documents contain both commands.

**Step 3: Run all final gates**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
cargo test --workspace --doc --locked
pnpm --dir web install --frozen-lockfile --offline
pnpm --dir web format:check
pnpm --dir web lint
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
git diff --check
```

Expected: all exit 0 and no generated build output is tracked.

**Step 4: Commit**

```bash
git add README.md docs/development.md
git commit -m "docs: document Rust and Oxc quality gates"
```

### Task 5: Final review

**Files:**

- Review: all files changed since `2068ba4`

**Step 1: Mechanically verify registrations**

Confirm:

- root `rustfmt.toml` and `clippy.toml` exist;
- all 13 workspace crates inherit workspace lints;
- all four Clippy groups are denied;
- Oxc dependencies are exact and locked;
- Oxfmt/Oxlint schemas resolve;
- package scripts exist and execute;
- no React-specific lint plugin is enabled for Solid;
- README and development docs contain the new commands.

**Step 2: Review the complete diff**

Run:

```bash
git diff --check 2068ba4..HEAD
git log --oneline 2068ba4..HEAD
git status --short --branch
```

Expected: focused commits, no unstaged files, and no whitespace errors.
