# Folder Ingestion Rules Design

**Date:** 2026-09-04
**Status:** Approved

## Goal

Turn Fixer into a user-facing media organizer built around persistent source-to-destination folder rules. Users select directories in the UI, Fixer scans existing media, watches for changes, scrapes metadata, and organizes high-confidence matches automatically. Ambiguous or unsafe work remains reviewable.

The UI must stop exposing the internal `Workspace` concept, remove unnecessary explanatory copy, and never require users to type filesystem paths.

## Product model

The primary configuration unit is an ingestion rule:

- a user-provided name;
- a source directory (`src`);
- a destination directory (`dst`);
- either one fixed media kind or automatic media-kind detection;
- one explicitly selected organization method: move, copy, hard link, symbolic link, or reflink;
- a built-in path-template preset selected by media kind, with an optional rule-level override;
- an enabled or disabled state.

Organization method is not a global setting and has no default. A rule cannot be saved until the user selects it. Existing command-scoped CLI placement options may remain explicit per invocation, but Web jobs and watched-folder jobs carry their own placement choice.

## User interface

Rename the user-facing `Workspace` surface to `Overview`. Internal Rust or TypeScript names may remain where renaming would not change user behavior.

The application navigation becomes centered on:

- `Overview`: service state, pending review count, failures, and recent activity;
- `Folders`: ingestion-rule list and rule editor;
- `Jobs`: processing history and review queue;
- the existing search, provider, template, and settings tools where still useful.

The `Folders` rule editor uses the existing server-backed safe directory browser for both source and destination. The browser exposes configured root labels and root-relative paths only. It does not expose an absolute-path text field. Users cannot browse outside roots configured by the administrator.

Page headers and empty states use short operational labels. Long descriptions about implementation details, safety policy, or the meaning of a workspace are removed from primary screens. Detailed failures remain available in job details.

The one-off job form also uses the directory picker instead of a path text field and requires an explicit destination and organization method when it will place media.

## Directory references and safety

Browser-facing APIs represent a directory as an opaque configured root ID plus a normalized relative path. The server resolves this reference against its canonical root allowlist before storing or using it. Absolute paths stay server-side.

A rule is rejected when:

- either directory is missing, inaccessible, or outside the configured roots;
- the source and destination are equal;
- one directory contains the other;
- the organization method is absent;
- a custom template is invalid or can escape the destination;
- the fixed media kind is unsupported.

If an administrator later removes a configured root, affected rules become disabled with an actionable error instead of resolving against another root.

## Persistence and API

Store ingestion rules and processed-source records in SQLite alongside jobs. Rules survive restarts and can be listed, created, updated, enabled, disabled, deleted, and manually rescanned through authenticated, CSRF-protected API endpoints.

Each processed-source record associates a rule with a stable source identity, the observed size and modification time, the resulting job, and its last outcome. This provides restart-safe deduplication and an audit trail without hashing complete media files.

A job created from a rule stores an immutable snapshot of:

- the source object;
- resolved destination directory;
- media-kind mode and detected media kind;
- organization method;
- selected path template;
- originating rule ID.

Editing a rule never changes work already queued or awaiting review.

## Discovery and watching

Use a hybrid watcher:

1. Recursively discover existing content when an enabled rule starts.
2. Consume filesystem create, modify, and rename events for low-latency updates.
3. Run a periodic reconciliation scan to recover events missed during downtime, event overflow, container remounts, or network-filesystem behavior.
4. Debounce a candidate until its size and modification time remain stable before reading or organizing it.
5. Compare the source fingerprint with persisted processing records before creating work.

The watcher ignores its destination tree and Fixer temporary files. Source and destination overlap is forbidden to prevent feedback loops.

Directory discovery emits one logical media item per job rather than passing a multi-title directory to the current single-item worker. Fixed-kind rules use only the selected scanner. Automatic rules run bounded local identification and either choose one media kind or create review work when identification is ambiguous.

Watcher failures do not stop the server. A rule records a concise error, retries through reconciliation, and resumes when its directory becomes available.

## Scrape and organization flow

For each discovered item:

1. Scan local metadata and infer or apply the media kind.
2. Search providers and resolve candidates.
3. Build a destination package path from the rule's media-specific preset or custom template.
4. Produce an inspectable filesystem plan using the rule's organization method.
5. Automatically approve and execute only when there is one eligible match at or above `auto_accept_confidence`, no unresolved metadata conflict, no destination collision, and a valid bounded plan.
6. Send low-confidence, multi-candidate, conflicting, or unsafe work to the existing review flow.

Automatic acceptance uses the configured confidence threshold as behavior, not merely as reported configuration. Automatic execution is limited to jobs created by an enabled ingestion rule; manual jobs retain explicit review controls unless the user chooses otherwise in that flow.

## Filesystem semantics

All organization modes are explicit and never silently degrade into another mode:

- `move`: rename on one filesystem; across filesystems, copy to a destination temporary file, finalize it atomically, then remove the source only after success;
- `copy`: copy to a destination temporary file and atomically finalize it;
- `hardlink`: create a hard link and report unsupported cross-device operations;
- `symlink`: create an explicit symbolic link according to the planned target;
- `reflink`: clone when supported and report unsupported filesystems.

Destination collisions never overwrite existing media automatically. Failed operations preserve the source and leave the job inspectable. Metadata and sidecar writes continue to use bounded plans and confirmation-aware execution.

## Destination templates

Provide built-in organization presets for movie, television, anime, music, and book destinations. A fixed-kind rule uses that kind's preset unless the user overrides it. An automatic rule selects the preset after detection.

Custom templates are rule-scoped, relative to `dst`, validated before saving, and rendered again when planning. They cannot contain absolute paths or parent traversal. Template preview is embedded in the rule editor rather than requiring users to understand a separate developer-oriented template playground.

## Error handling

The rule list exposes only operational states: watching, processing, needs review, paused, and error. Job details contain the specific failure and request ID where applicable.

Filesystem event loss is repaired by reconciliation. Temporary source unavailability pauses processing without deleting state. Link or clone capability failures do not trigger a copy fallback. Destination conflicts and uncertain detection require review. API validation errors identify the invalid field without exposing canonical server paths.

## Verification

Backend tests cover:

- rule validation and persistence;
- directory-reference resolution and root removal;
- overlap and traversal rejection;
- initial recursive discovery, event-driven discovery, and reconciliation;
- stability debounce and restart-safe deduplication;
- fixed and automatic media-kind discovery;
- confidence-gated automatic execution;
- destination collisions and all five organization methods;
- source preservation on failed copy or cross-filesystem move.

Web tests cover:

- no user-visible `Workspace` wording;
- concise Overview, Folders, and Jobs surfaces;
- keyboard-accessible source and destination pickers;
- absence of manual filesystem path inputs;
- required organization-method selection;
- fixed and automatic media-kind modes;
- preset selection, custom-template preview, and validation;
- rule lifecycle and pending-review navigation.

A browser acceptance flow creates a rule against temporary configured roots, discovers an existing media item, verifies high-confidence automatic organization into the destination, and verifies that an ambiguous item stops for review.
