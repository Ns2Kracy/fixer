# Scraping repair — 2026-09-07

## Approved scope / plan

1. Reproduce directory episode layout, selected-candidate enrichment, TMDB v3 authentication, and failed-enrichment auto-execution regressions (RED).
2. Route recognized television episodes by their own season, retarget episode NFOs to media stems, and colocate uniquely matched subtitles. Reject ambiguous episode/target mappings; preserve unrelated files and other media layouts.
3. Centralize TMDB JSON requests: 32-hex v3 API keys use `api_key`; other tokens retain v4 Bearer auth. Remove credential-bearing URLs from transport failures.
4. Keep compatible cross-provider candidates after explicit selection; exclude conflicting IDs/years, unrelated titles, and ambiguous provider matches. Preserve selected identity and metadata/artwork URL enrichment.
5. Require review on failed provider search/fetch or truncated diagnostics; carry the public reason through Rust and TypeScript.
6. Run focused writer/provider/HTTP/SDK/server tests and record results.

No production config/database, movie-library edits, commit/push, artwork downloads, or durable Job audit are in scope.

## Acceptance ledger

- [x] Regression coverage: flat and nested television episodes use their detected `Season NN`; generated episode NFOs and unique subtitles follow the media target.
- [x] Rich metadata coverage: TMDB v3/v4 authentication, localized summaries, seasons, episodes, credits, posters/stills, and local+TMDB merge are exercised with offline fixtures.
- [x] Safety coverage: ambiguous episode mappings fail closed; provider enrichment failures require review; copy/hardlink paths preserve source media.
- [x] Verification: `cargo fmt --check`, workspace clippy with all targets/features, and `cargo test --workspace` pass.

No live TMDB request or production media write was used for verification. Durable per-file Job audit artifacts remain a separate backend slice.
