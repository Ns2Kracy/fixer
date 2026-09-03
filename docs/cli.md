# Fixer CLI

The `fixer` binary searches providers, resolves metadata, scans local files, prepares output plans, and applies plans for movies, television, anime, music, and books.

Run the development binary with:

```bash
cargo run -p fixer-cli -- --help
```

Global options may appear before or after a subcommand:

```text
--config FILE
--local-root PATH
--offline
--proxy URL
```

`search` and `resolve` currently require `--local-root` or `FIXER_LOCAL_ROOT`, even when a network provider is enabled. The CLI builds one local provider from that root and adds configured network providers.

## Workflow map

| Workflow | Movie | Television | Anime | Music | Book |
| --- | ---: | ---: | ---: | ---: | ---: |
| `search` | yes | yes | yes | yes | yes |
| `resolve` | yes | yes | yes | yes | yes |
| `scan` | yes | yes | yes | yes | yes |
| `plan` | yes | yes | yes | yes | yes |
| `scrape` | yes | yes | yes | yes | yes |

All media kinds support in-place planning and scraping. Movie and television also support media placement. Anime, music, and book reject non-in-place placement because those workflows do not yet identify one relocatable media file safely.

## Search

```bash
fixer --local-root ./library --offline search movie "In the Mood for Love" --year 2000
fixer --local-root ./library search television "Example Show" --ordering aired
fixer --local-root ./library search anime "Frieren" --external-id anilist:154587
fixer --local-root ./library search music "Kind of Blue" --year 1959
fixer --local-root ./library search book "The Left Hand of Darkness" --isbn 9780441478125
```

Search writes ranked candidates as text. Anime and television accept repeatable `--external-id NAMESPACE:ID`. Book validates ISBN-13 before provider search.

## Resolve

`resolve` searches and merges provider metadata. Movie resolution fetches every ranked candidate; music and book fetch only the deterministic top candidate; television and anime narrow ranked results to one compatible cross-provider group. Add `--json` for the stable versioned DTO.

```bash
fixer --local-root ./library --offline resolve movie "Arrival" --year 2016 --json
fixer --local-root ./library --offline resolve book "The Left Hand of Darkness" \
  --isbn 9780441478125 --json
```

Every resolve DTO in schema version `1` has these fields:

| Field | JSON type | Contract |
| --- | --- | --- |
| `schema_version` | integer | Always `1` for the shapes in this section. |
| `kind` | string | `anime`, `book`, `movie`, `music`, or `television`. |
| `id` | string | Resolved work/release-group/series identity. |
| `title` | string | CLI-selected display title. |
| `completeness` | number | Resolved completeness score. |
| `conflicts` | integer | Number of merge conflicts. |
| `warnings` | array of strings | Human-readable non-fatal resolution warning messages. |

Media-specific top-level fields are:

| `kind` | Additional fields |
| --- | --- |
| `movie` | `year: integer or null`, `titles: Title[]` |
| `television` | `ordering: "aired" \| "dvd" \| "absolute"`, `seasons: integer`, `episodes: integer`, `titles: Title[]` |
| `anime` | `relation: "original" \| "adaptation" \| "sequel" \| "prequel" \| "side_story" \| "spin_off"`, `cours: integer`, `episodes: integer`, `titles: Title[]` |
| `music` | `artist: string`, `releases: MusicRelease[]` |
| `book` | `contributors: BookContributor[]`, `editions: BookEdition[]` |

Nested version 1 objects have these exact fields:

| Object | Fields |
| --- | --- |
| `Title` | `locale: string or null`, `value: string` |
| `BookContributor` | `id: string`, `name: string`, `role: string` (`director`, `writer`, `actor`, `author`, `editor`, `translator`, `performer`, `composer`, or `producer`) |
| `BookEdition` | `id: string`, `isbn_10: string`, `isbn_13: string`, `publisher: string`, `assets: integer` |
| `MusicRelease` | `id: string`, `discs: MusicDisc[]` |
| `MusicDisc` | `number: integer`, `tracks: MusicTrack[]` |
| `MusicTrack` | `id: string`, `title: string`, `disc: integer`, `track: integer`, `duration_seconds: integer` |

A complete movie object has this shape:

```json
{
  "schema_version": 1,
  "kind": "movie",
  "id": "movie-arrival",
  "title": "Arrival",
  "year": 2016,
  "titles": [
    {
      "locale": "en",
      "value": "Arrival"
    }
  ],
  "completeness": 0.8,
  "conflicts": 0,
  "warnings": []
}
```

Counts and durations are non-negative JSON integers. CLI DTOs do not serialize internal SDK structs directly; automation should branch on `schema_version` and `kind` before reading media-specific fields.

## Scan

`scan` invokes only the selected local scanner. It does not contact network providers, resolve candidates, construct writer output, or modify files.

```bash
fixer scan ./library --kind television
fixer scan ./library --kind book --json
```

If `PATH` is a file, the scanner uses its parent directory as the scan root. JSON uses this stable summary shape:

```json
{
  "schema_version": 1,
  "kind": "book",
  "root": "/absolute/library/path",
  "documents": 2,
  "warnings": [
    {
      "path": "/absolute/library/path/broken.epub",
      "message": "local EPUB container was invalid: ..."
    }
  ]
}
```

The summary reports counts and structured warnings; it does not expose domain model snapshots.

## Plan

`plan` runs local identification, provider resolution, merging, and the selected writer. It returns before the output executor, so it cannot write even if targets do not exist.

```bash
fixer --offline plan ./library/Arrival.2016.mkv --kind movie --json
fixer --offline plan ./shows/Example.Show.S01E01.mkv \
  --kind television --placement hardlink --json
```

`plan` does not accept `--apply`, `--dry-run`, or `--update-epub`. JSON contains byte-redacted operations:

```json
{
  "schema_version": 1,
  "kind": "movie",
  "output_root": "/library/Arrival (2016)",
  "operations": [
    {
      "operation": "write_bytes",
      "target": "movie.json"
    },
    {
      "operation": "copy",
      "source": "/library/Arrival.2016.mkv",
      "target": "Arrival.2016.mkv"
    }
  ]
}
```

Operation names are `create_directory`, `write_bytes`, `copy`, `symlink`, `hardlink`, and `reflink`. Planned byte content is intentionally omitted.

## Scrape

`scrape` uses the same resolver and writer pipeline as `plan` and then passes the prepared plan to the safe executor.

```bash
# Preview. This is also the default when neither flag is supplied.
fixer --offline scrape ./library/Arrival.2016.mkv --kind movie --dry-run

# Execute with no-overwrite behavior.
fixer --offline scrape ./library/Arrival.2016.mkv --kind movie --apply
```

`--dry-run` and `--apply` are mutually exclusive. Without `--apply`, the executor performs a dry run. Existing targets fail under the default no-overwrite policy.

Book scraping writes sidecars next to the selected EPUB. `--update-epub` requires one EPUB path and creates a confirmation-required mutation intent; the current writer does not alter the archive.

## Placement

`plan` and `scrape` accept:

```text
--placement in-place|symlink|hardlink|copy|reflink
```

The CLI flag overrides configured placement. Without the flag, `placement` or `FIXER_PLACEMENT` applies. The default is `in_place`.

Non-in-place movie and television placement requires a media file path. `symlink` creates a relative symlink. Reflink remains required when selected; the executor does not silently fall back to copy.

## Output and conflict policies

`output_preset` has two values:

- `full`: keep all writer-planned operations.
- `metadata`: keep writer-planned directory and metadata-byte writes, and omit writer-planned copy, symlink, hardlink, and reflink asset transfers.

Media placement is independent. The CLI applies the output preset first and appends an explicit placement operation afterward.

`conflict_policy` controls real merge conflicts:

- `prefer_first`: keep deterministic provider precedence and continue.
- `review`: emit the plan, return review-required status, and stop before execution. This is a blocking preview, not an interactive conflict resolver.
- `error`: reject the workflow as an execution failure.

## Providers and validation

```bash
fixer providers list
fixer config validate
fixer --config ./fixer.toml config validate
```

`providers list` reports providers compiled into the CLI and their media capabilities. `config validate` loads and validates the effective configuration, then prints a redacted policy summary. The summary reports whether Open Library endpoint overrides are configured; other endpoint overrides are validated but not listed. It never prints API tokens or resolved secret values.

## Exit codes

| Code | Meaning |
| ---: | --- |
| `0` | Complete success |
| `1` | Workflow, provider, planning, policy-error, or execution failure |
| `2` | Invalid CLI input or invalid configuration |
| `3` | Partial success with non-fatal scan or resolution warnings |
| `4` | Review required; unresolved conflicts prevented execution |

A review-required `scrape --apply` prints the planned operations and returns `4` without writing. Exit `3` can report a malformed sidecar, a skipped network provider in offline mode, an ambiguous top-ranked candidate, or a provider search/fetch failure when usable output still completes.

## Current limits

- Search output is text-only. Resolve, scan, and plan expose versioned JSON.
- Anime, music, and book support only in-place placement.
- `--update-epub` writes an intent sidecar and does not mutate EPUB content.
- Confidence thresholds are validated configuration values, but the CLI does not gate candidate selection on them yet. The SDK currently exposes integer matching evidence rather than a normalized unit-interval selection confidence.
- Review mode emits the ordinary output plan and blocks execution; it does not list field-level conflicts, select a candidate interactively, or accept conflict-resolution input. Restrict `enabled_providers`, change `conflict_policy`, or correct the conflicting metadata externally, then rerun.
- Local EPUB scanning loads the EPUB archive file into memory, then rejects any selected container or OPF metadata entry larger than 1 MiB. The whole archive currently has no separate size cap.
