# Work-centred organization UI implementation plan

## Approved design

Replace the eight peer navigation entries with 整理, 整理记录, 设置. Folders are source→destination automation rules, not works. Library is a file browser, not a media library. Jobs remain backend execution identities but are presented as works being organized. Preserve old URLs and safety gates.

Home creation starts with one action: select the folder to organize. The closest enabled Folder rule supplies detection mode, destination, placement and naming template. Recognition starts immediately, but manual rule-based jobs never auto-execute; writing still requires the existing final approval. If no rule matches, or automatic detection is ambiguous, reveal the compact settings only then and offer to remember them as a Folder rule.

## Acceptance ledger

- [x] Primary navigation has exactly the three approved destinations; configuration subpages stay reachable under Settings.
- [x] Home is a real organization inbox and creation flow, not a hard-coded zero-activity dashboard. Record list separates finished outcomes from active work and clearly discloses bounded listings.
- [x] Source and destination choices describe their roles. New work opens its detail after creation.
- [x] Work detail keeps candidate review and output approval inline; source path is secondary to the work name. Technical pipeline is collapsed by default.
- [x] Executed work shows persisted summary honestly; no reconstructed plan is presented as historical evidence. Persistent full audit remains a separate pending backend slice.
- [x] Existing auth, conflict acknowledgement, dry run, incomplete-plan and duplicate-execution guards remain enforced.
- [x] Targeted route tests, all web tests, typecheck and browser interaction checks pass.
- [x] Selecting a directory under an enabled Folder rule immediately creates one auditable Job from the most specific matching rule.
- [x] The server verifies the selected directory is inside that rule, inherits its organization settings, detects exactly one work and keeps `auto_execute` false. Repeated submissions return the existing Job.
- [x] Unmatched or ambiguous directories reveal compact settings; users can save those settings for later runs.

## Implementation slices

1. Add failing navigation/inbox/history tests in web/src/routes/organize.test.tsx.
2. Reuse JobsPage in home and records modes; simplify AppShell navigation and add contextual settings links. Preserve existing route URLs.
3. Extract parameterized ReviewPanel and PlanPanel from existing routes; embed in job details without route hopping. Keep legacy wrappers.
4. Update behavioral tests for new labels/flows, run pnpm test and pnpm typecheck; verify desktop/mobile using Playwright fixtures without touching the live library.
5. Replace the expanded home form with directory-first creation. Add the rule-based `POST /jobs` request variant, server-side discovery and source reservation so direct runs inherit configuration without racing the folder watcher.

Do not modify production configuration, database or media. Existing scraping repairs remain intact. No new dependencies. Do not claim rich metadata preview or durable audit until backend artifacts exist.
