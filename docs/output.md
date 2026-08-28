# Output planning and execution

Fixer separates planning from filesystem mutation. Writers produce a typed `fixer_core::OutputPlan`, and the SDK validates and prepares it before execution. CLI and server previews expose operation summaries, with the binding and truncation limits described below.

## CLI workflow

`plan` never writes:

```bash
fixer --offline plan ./library/Arrival.mkv --kind movie --json
```

`scrape` also previews by default. `--dry-run` makes that intent explicit; only `--apply` executes:

```bash
fixer --offline scrape ./library/Arrival.mkv --kind movie --dry-run
fixer --offline scrape ./library/Arrival.mkv --kind movie --apply
```

`--dry-run` and `--apply` conflict. Review-required merge policy returns exit code `4` before execution even when `--apply` was requested.

A standalone `fixer plan` invocation is not an approval token for a later command. `fixer scrape --apply` scans, resolves, and plans again, then prepares and executes that new plan. Inputs or provider results can change between the two commands. Use `scrape --dry-run` close to execution, keep source/provider state stable, and review the new command output; the CLI does not persist a plan fingerprint across invocations.

Plan JSON uses schema version `1`:

```json
{
  "schema_version": 1,
  "kind": "movie",
  "output_root": "/media/Arrival",
  "operations": [
    {
      "operation": "write_bytes",
      "target": "movie.json"
    }
  ]
}
```

The stable operation names are `create_directory`, `write_bytes`, `copy`, `symlink`, `hardlink`, and `reflink`. Write content is omitted from the CLI plan DTO.

## Writer outputs

Writers plan deterministic local artifacts without performing I/O:

| Writer | Planned artifacts |
| --- | --- |
| `JsonWriter` | movie `movie.json` |
| `NfoWriter` | movie `movie.nfo`; television hierarchy when given a television document |
| `TelevisionWriter` | `tvshow.nfo`, season NFOs, and episode NFOs |
| `AnimeWriter` | `anime.nfo`, cour NFOs, and episode NFOs |
| `MusicWriter` | `album.json`, `fixer-manifest.json`, and SDK-only optional `tag-update-intent.json` when constructed with explicit tag targets |
| `BookWriter` | `book.opf`, `book.json`, `fixer-manifest.json`, optional cover output/acquisition intent, and optional `epub-mutation-intent.json` |
| `ManifestWriter` | movie `fixer-manifest.json` with field provenance and named planned files |

The current CLI composes `JsonWriter` for movies, hierarchy writers for television/anime, `MusicWriter` for music, and `BookWriter` for books. A movie provenance manifest is available through the SDK's `ManifestWriter`; the CLI movie path does not add one automatically.

Remote book artwork produces `cover-acquisition-intent.json`; the writer does not download it. `--update-epub` produces a confirmation-required intent file and does not rewrite the EPUB. Rust callers can construct `MusicWriter::with_tag_targets` to produce a no-mutation tag intent. The CLI and server currently use `MusicWriter::default()` and expose no tag-target input, so they do not produce `tag-update-intent.json`.

## Templates

`PathTemplate` and `ContentTemplate` expose only these variables:

```text
title
year
edition
id
```

Supported filters are `sanitize`, `lower`, and `upper`:

```rust
use fixer_writer_local::{PathTemplate, TemplateContext};

# fn render() -> Result<(), fixer_writer_local::TemplateError> {
let context = TemplateContext::preview("In the Mood for Love", "movie-843", Some(2000), None)?;
let template = PathTemplate::new("{{ title | sanitize }} ({{ year }})")?;
assert_eq!(template.render(&context)?.to_string_lossy(), "In the Mood for Love (2000)");
# Ok(())
# }
```

Path output must remain relative and rejects traversal, absolute paths, unsafe components, control characters, and platform-reserved punctuation. Content templates share the bounded variable/filter parser but return text. Templates execute no arbitrary code and perform no I/O.

The CLI uses `{{ title | sanitize }} ({{ year }})` for non-in-place movie folders when a year exists. The server exposes authenticated `POST /api/v1/templates/preview` for no-write path/content previews.

## Provenance manifests

`fixer-manifest.json` schema version `1` records the resolved work identity, field-level provider provenance where available, and planned file names. Book manifests also identify the selected edition and ISBN. Music and book manifests are part of their normal writer plans.

Manifests do not contain HTTP authorization headers or plaintext provider credentials. They can contain provider IDs, external metadata IDs, locales, and local planned paths; treat those as library metadata when sharing a manifest.

The CLI's `metadata` output preset removes planned asset transfer operations and reconciles named `planned_files` entries in a manifest. `full` keeps every writer-planned operation. An explicit movie/television media placement operation is appended after preset filtering and remains independent of the preset.

## SDK execution

```rust
use fixer_sdk::output::{ExecutionPolicy, OutputPlanExt};

# fn execute(plan: fixer_core::OutputPlan) -> Result<(), Box<dyn std::error::Error>> {
let prepared = plan.prepare()?;
let operation_count = prepared.preview().operations().len();
for operation in prepared.preview().operations() {
    println!("target: {}", operation.target().unwrap().display());
}

let report = prepared.execute(ExecutionPolicy::default())?;
assert_eq!(report.operations().len(), operation_count);
# Ok(())
# }
```

`OutputPlanExt::preview()` validates a borrowed plan without mutation. `prepare()` consumes the plan and records source/target fingerprints; inspect `PreparedOutputPlan::preview()` before calling its `execute()` method.

`ExecutionPolicy::default()` uses `OverwritePolicy::NoOverwrite` and `ReflinkPolicy::Required`. `ExecutionPolicy::dry_run()` fingerprints and validates paths but reports `DryRun` without mutation.

## Safety and atomicity

Preparation and execution enforce these boundaries:

- operation targets must be safe relative paths under the output root;
- symlinked output ancestors that could escape the root are rejected;
- source and target fingerprints are captured at preparation and checked again before execution;
- existing file-like targets fail under the default no-overwrite policy; an existing directory satisfies `create_directory`;
- write/copy/reflink bytes are created in a unique temporary file, synced for byte writes, then published at the target;
- no-overwrite publication uses a hardlink from the temporary file to the final name, so even `write_bytes`, `copy`, and reflink workflows require target-filesystem hardlink support;
- temporary artifacts are removed after publication failures where possible.

Under default no-overwrite, each byte/copy/reflink target is published as one complete file. Atomicity is not plan-wide. A plan executes operations in order and stops at the first error. `ExecutionFailure::report()` lists completed and failed operations, and completed earlier operations remain on disk. There is no whole-plan rollback. Directories created earlier can also remain. Review the report before retrying; no-overwrite and stale fingerprints protect against silently repeating completed work.

`OverwritePolicy::Replace` is an explicit SDK option. On non-Windows platforms it renames the temporary file over the target. On Windows, replacement can remove the old target before a second rename; failure after removal has no rollback. The CLI and server use default no-overwrite and expose no replace flag.

## Placement semantics

Movie and television workflows support media placement. Anime, music, and book currently require `in_place` because those workflows do not identify one safely relocatable media file.

| Mode | Behavior | Storage and portability |
| --- | --- | --- |
| `in-place` | Writes metadata beside the selected media hierarchy; no media placement operation. | No duplicate media bytes. |
| `symlink` | CLI creates a relative symlink from the organized target to the source. SDK also exposes absolute symlinks. | Small and movable only when source/target relative layout stays intact. Support and privileges vary by OS and filesystem. |
| `hardlink` | Creates another directory entry for the same inode/file identity. | No duplicate data; source and target must support hardlinks, commonly the same filesystem. |
| `copy` | Copies media bytes to a new independent file. | Portable but consumes full additional space. |
| `reflink` | Requests a copy-on-write clone. | Fast and space-efficient when the filesystem supports it; otherwise policy decides whether to fail or copy. |

### Hardlink mutation caveat

A hardlink is not an independent copy. In-place mutation through any hardlinked path changes the bytes visible through every path sharing that file identity. Review all hardlinked paths before enabling tag or container mutation. Current music and EPUB writers emit mutation intent files rather than modifying source media.

Replacing one hardlink path with a newly published file changes that directory entry, but ordinary writes through an existing hardlink still affect every link to the inode.

### Symlink portability

The CLI uses relative symlinks so moving the complete source/target tree together can preserve the relation. Moving only one side can break it. Archive tools, SMB/NFS mounts, Windows policy, containers, and media servers may handle symlinks differently. Test the deployed filesystem and consumer before choosing this mode.

### Reflink fallback

The CLI uses `ReflinkPolicy::Required`; unsupported reflinks fail and do not silently become full copies.

SDK callers can opt in:

```rust
use fixer_sdk::output::{ExecutionPolicy, ReflinkPolicy};

let policy = ExecutionPolicy::default().with_reflink(ReflinkPolicy::FallbackToCopy);
```

Fallback applies to an unsupported reflink result after source/target validation. Missing sources, permission denial, and temporary target collisions remain errors. A successful fallback reports `OperationStatus::CopiedFallback`, allowing callers to detect the extra storage cost.

## Server filesystem boundary

The production server adds `FsPolicy` on top of SDK plan validation. It canonicalizes configured media roots, validates reads and writes under those roots, checks the nearest existing ancestor for new targets, and revalidates every plan immediately before execution. See [Server operations](server.md) and [Security model](security.md).
