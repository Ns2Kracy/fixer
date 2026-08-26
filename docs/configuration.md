# Fixer configuration

Fixer reads JSON configuration, environment variables, and global CLI flags. Configuration is validated before command dispatch.

## File discovery and precedence

The CLI chooses one file in this order:

1. `--config FILE`
2. `FIXER_CONFIG`
3. `./fixer.json`, when present
4. built-in defaults

For settings with a global CLI flag, precedence is flag, environment, file, default. Those settings are `offline`, `proxy`, and `local_root`. Other settings use environment, file, default.

Run:

```bash
fixer --config ./fixer.json config validate
```

The command prints effective non-secret policy, secret-reference status, and whether Open Library endpoint overrides are configured. Other provider endpoint overrides are validated but not listed in the summary. Secret values are redacted.

## Complete example

```json
{
  "offline": false,
  "proxy": "http://127.0.0.1:7890",
  "local_root": "/media/library",
  "preferred_locales": ["zh-Hans", "zh-Hant", "ja", "en", "und"],
  "timeout_seconds": 30,
  "auto_accept_confidence": 0.9,
  "review_confidence": 0.6,
  "output_preset": "full",
  "placement": "in_place",
  "conflict_policy": "review",
  "enabled_providers": [
    "local",
    "tmdb",
    "bangumi",
    "musicbrainz",
    "openlibrary"
  ],
  "tmdb_base_url": "https://api.themoviedb.org/3",
  "bangumi_base_url": "https://api.bgm.tv",
  "musicbrainz_base_url": "https://musicbrainz.org/ws/2",
  "openlibrary_base_url": "https://openlibrary.org",
  "openlibrary_cover_base_url": "https://covers.openlibrary.org/b/",
  "anilist_enabled": false,
  "anilist_endpoint": "https://graphql.anilist.co",
  "secret_references": {
    "tmdb_api_token": "FIXER_TMDB_TOKEN",
    "anilist_access_token": "FIXER_ANILIST_TOKEN"
  }
}
```

Unknown fields fail validation.

## General settings

| JSON field | Environment | Default | Validation and behavior |
| --- | --- | --- | --- |
| `offline` | `FIXER_OFFLINE` | `false` | Boolean. Skips providers that require network access. |
| `proxy` | `FIXER_PROXY` | unset | Global HTTP proxy used by the default SDK transport. |
| `local_root` | `FIXER_LOCAL_ROOT` | unset | Required by current `search` and `resolve` commands. |
| `preferred_locales` | `FIXER_PREFERRED_LOCALES` | `zh-Hans,zh-Hant,ja,en,und` | Ordered BCP 47 tags. Environment value is comma-separated. Empty or malformed lists fail validation. |
| `timeout_seconds` | `FIXER_TIMEOUT_SECONDS` | `30` | Positive integer passed to the default HTTP transport. Zero fails validation. |
| `auto_accept_confidence` | `FIXER_AUTO_ACCEPT_CONFIDENCE` | `0.9` | Finite value in `0.0..=1.0`. Must be at least `review_confidence`. |
| `review_confidence` | `FIXER_REVIEW_CONFIDENCE` | `0.6` | Finite value in `0.0..=1.0`. Must not exceed `auto_accept_confidence`. |
| `output_preset` | `FIXER_OUTPUT_PRESET` | `full` | `full` or `metadata`; see below. |
| `placement` | `FIXER_PLACEMENT` | `in_place` | `in_place`, `symlink`, `hardlink`, `copy`, or `reflink`. An explicit CLI flag wins. |
| `conflict_policy` | `FIXER_CONFLICT_POLICY` | `review` | `prefer_first`, `review`, or `error`. |
| `enabled_providers` | `FIXER_ENABLED_PROVIDERS` | see provider table | Ordered provider allowlist. Environment value is comma-separated. Unknown or empty lists fail validation. |

Boolean environment values accept `1`, `true`, `yes`, `on`, `0`, `false`, `no`, and `off`, without case sensitivity.

### Confidence threshold status

The CLI validates and reports both confidence thresholds so the schema can remain stable across adapters. It does not currently apply them to candidate selection. The matcher exposes integer evidence scores, not a normalized `0.0..=1.0` selection confidence. Merge conflicts use `conflict_policy` today.

### Output presets

`full` keeps every operation produced by the selected writer.

`metadata` keeps writer-planned directory creation and byte writes. It drops writer-planned copy, symlink, hardlink, and reflink operations used for local assets. The CLI applies media placement after this filter, so `placement: copy` still copies the selected media file.

### Conflict policy

| Value | Result when merge conflicts exist |
| --- | --- |
| `prefer_first` | Continue with deterministic provider precedence. |
| `review` | Emit the ordinary output plan, return exit code `4`, and stop before execution. `plan --json` provides JSON; `scrape` prints text. This is not an interactive conflict resolver. |
| `error` | Return exit code `1` with a safe conflict count. |

## Providers

| ID | Media | Network | Enabled by default |
| --- | --- | ---: | ---: |
| `local` | movie, television, anime, music, book | no | yes |
| `tmdb` | movie, television | yes | yes, but inactive without a token |
| `bangumi` | anime | yes | yes |
| `anilist` | anime | yes | no |
| `musicbrainz` | music | yes | yes |
| `openlibrary` | book | yes | yes |

Set `enabled_providers` to restrict registration. Duplicates are removed while preserving first occurrence order. If no explicit allowlist exists, legacy `anilist_enabled: true` adds AniList. With an explicit allowlist, include `anilist` to enable it.

`anilist_enabled` and `FIXER_ANILIST_ENABLED` remain supported for compatibility. Prefer `enabled_providers` for new configuration.

## Provider endpoints and credentials

| JSON field | Environment | Purpose |
| --- | --- | --- |
| `api_key` or `tmdb_api_token` | `TMDB_API_TOKEN`, then `FIXER_API_KEY` | TMDB bearer token. `tmdb_api_token` is a file alias for compatibility. |
| `tmdb_base_url` | `TMDB_BASE_URL` | TMDB API endpoint override. |
| `bangumi_base_url` | `BANGUMI_BASE_URL` | Bangumi API endpoint override. |
| `musicbrainz_base_url` | `MUSICBRAINZ_BASE_URL` | MusicBrainz API endpoint override. |
| `openlibrary_base_url` | `OPENLIBRARY_BASE_URL` | Open Library API endpoint override. |
| `openlibrary_cover_base_url` | `OPENLIBRARY_COVER_BASE_URL` | Open Library Covers endpoint override. |
| `anilist_endpoint` | `ANILIST_ENDPOINT` | AniList GraphQL endpoint override. |
| `anilist_access_token` | `ANILIST_ACCESS_TOKEN` | Optional AniList bearer token. |

Endpoint overrides are useful for sanitized fixture servers and controlled mirrors. Fixer validates each endpoint through its provider configuration before use. Open Library cover overrides must include the path prefix onto which `id/...` is joined; the production service uses `https://covers.openlibrary.org/b/`.

## Secret references

Prefer environment-backed references over embedding tokens in JSON:

```json
{
  "secret_references": {
    "tmdb_api_token": "FIXER_TMDB_TOKEN",
    "anilist_access_token": "FIXER_ANILIST_TOKEN"
  }
}
```

A reference must contain only uppercase ASCII letters, digits, and underscores. The named environment variable must exist when Fixer loads the file. `config validate` reports `configured` without printing the variable value.

Direct credential precedence is:

- TMDB: `TMDB_API_TOKEN`, `FIXER_API_KEY`, referenced environment variable, file value.
- AniList: `ANILIST_ACCESS_TOKEN`, referenced environment variable, file value.

Do not commit direct `api_key`, `tmdb_api_token`, or `anilist_access_token` values.

## CLI overrides

The following global flags override their environment and file counterparts:

```text
--offline
--proxy URL
--local-root PATH
```

`--config FILE` selects the configuration file. Command-level `--placement` overrides configured placement for that invocation.

## Validation failures

Configuration load failures return exit code `2`. Common causes include:

- unknown JSON fields or provider IDs;
- malformed BCP 47 locale tags;
- zero timeout;
- thresholds outside `0.0..=1.0`;
- `review_confidence` above `auto_accept_confidence`;
- invalid enum values;
- empty provider lists;
- malformed or unset secret references;
- malformed provider endpoints or proxy URLs.

## Safety limits

Local EPUB scanning loads the EPUB archive file into memory and parses only the selected container and OPF metadata entries. It rejects any selected metadata entry larger than 1 MiB and reports that book as a warning, but the whole archive currently has no separate size cap.
