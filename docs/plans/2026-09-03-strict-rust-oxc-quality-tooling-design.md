# Strict Rust and Oxc Quality Tooling Design

**Date:** 2026-09-03
**Status:** Approved

## Context

Fixer currently relies on implicit rustfmt defaults and invokes Clippy with `-D warnings`, but it does not commit formatter policy, Clippy tuning, or workspace lint inheritance. The Solid frontend has strict TypeScript settings but no committed formatter or linter. Recent editor-driven formatting showed that implicit tool defaults can produce inconsistent output.

The repository needs one reproducible quality policy for Rust and one for the `web` package. The policy should be strict enough to reject correctness, suspicious, pedantic, performance, and package-quality regressions without enabling mutually contradictory restriction rules.

## Decision

Adopt a strict but maintainable baseline:

- Commit root `rustfmt.toml` and `clippy.toml` files.
- Enable `clippy::all`, `clippy::pedantic`, `clippy::nursery`, and `clippy::cargo` at `deny` level through workspace lint inheritance.
- Do not enable the complete `clippy::restriction` group. Clippy explicitly warns that the group contains contradictory lints; individual restriction lints may be selected later when they express a project policy.
- Add Oxfmt and Oxlint to the `web` package, commit their configuration files, and expose deterministic package scripts for local development and CI.
- Make every new gate pass before the tooling change is committed. Any exception must name a specific lint and explain why it is subjective, incompatible, or inapplicable.

## Rust formatting

`rustfmt.toml` will use stable rustfmt 1.9 options only:

- Rust edition and style edition `2024`;
- Unix newlines;
- the standard 100-column width;
- field-init and `?` shorthand where rustfmt can apply them safely.

The configuration will avoid nightly-only import grouping, comment wrapping, and other unstable settings so the declared MSRV and stable toolchain remain usable. `cargo fmt --all -- --check` remains the authoritative gate.

## Rust linting

`clippy.toml` will declare the workspace MSRV (`1.85`) and preserve public API compatibility when Clippy considers machine-applicable rewrites. Lint levels belong in `Cargo.toml`, because `clippy.toml` configures lint behavior but cannot enable lint groups.

The root manifest will define workspace lint levels with negative group priorities so explicitly named exceptions can override a group. Every member crate will opt in with:

```toml
[lints]
workspace = true
```

The baseline groups are:

```toml
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "deny", priority = -1 }
nursery = { level = "deny", priority = -1 }
cargo = { level = "deny", priority = -1 }
```

Existing violations will be fixed when the change improves clarity or correctness. Global exceptions are reserved for rules that are broadly unsuitable for this workspace, such as dependency-graph duplication outside Fixer's control or documentation-volume policies. Narrow false positives should use a local `#[allow]` with a reason rather than weakening the workspace.

## Frontend formatting

The `web` package will pin Oxfmt and commit `web/.oxfmtrc.json`. The configuration will retain the current Prettier-compatible style: two-space indentation, semicolons, double quotes, trailing commas, LF newlines, and deterministic wrapping. Generated output and dependency/build directories will be ignored.

Oxfmt's Solid/TSX parser support is sufficient without React-specific configuration. Optional sorting features will only be enabled when the installed schema confirms they are stable and they do not reorder side-effect imports.

The package will expose:

- `pnpm --dir web format` to write formatting;
- `pnpm --dir web format:check` to verify formatting without mutation.

## Frontend linting

The `web` package will pin Oxlint and `oxlint-tsgolint`, then commit `web/.oxlintrc.json` using the package-provided schema. Type-aware linting will be enabled; experimental Oxlint type checking will remain disabled because `tsc --noEmit` is already the authoritative type checker.

The configuration will enable stable correctness, suspicious, pedantic, and performance categories and the built-in TypeScript, Oxc, Unicorn, Import, Promise, JSX accessibility, and Vitest plugins. React and React Hooks rules will remain disabled because Solid's reactive and JSX semantics are not React's. Test-specific Vitest rules and environments will be scoped through overrides. Generated route trees and build artifacts will be ignored.

The package will expose `pnpm --dir web lint`. Lint errors are blocking; any compatibility exceptions will be explicit and documented in the config.

## Verification

The completed tooling must pass:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
pnpm --dir web format:check
pnpm --dir web lint
pnpm --dir web typecheck
pnpm --dir web test
pnpm --dir web build
```

Configuration files will also be parsed by their installed tools, and a mechanical scan will confirm that all workspace crates inherit the Rust lint policy.

## Sources

- Clippy configuration: <https://doc.rust-lang.org/clippy/configuration.html>
- Clippy lint groups: <https://doc.rust-lang.org/clippy/usage.html>
- Cargo lint inheritance: <https://doc.rust-lang.org/cargo/reference/manifest.html#the-lints-section>
- rustfmt configuration: <https://rust-lang.github.io/rustfmt/?version=v1.9.0>
- Oxlint configuration reference: <https://oxc.rs/docs/guide/usage/linter/config-file-reference.html>
- Oxlint type-aware linting: <https://oxc.rs/docs/guide/usage/linter/type-aware.html>
- Oxlint built-in plugins: <https://oxc.rs/docs/guide/usage/linter/plugins>
- Oxfmt: <https://oxc.rs/docs/guide/usage/formatter.html>
