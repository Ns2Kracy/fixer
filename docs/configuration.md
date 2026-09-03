# Fixer configuration

The CLI and `fixer-server` use the same `fixer_runtime::ConfigLoader` and the same
`FixerConfig` schema. The canonical configuration file is `fixer.toml`.
Configuration is validated before command dispatch, listener binding, or worker
startup.

Start from the committed template:

```bash
cp fixer.toml.example fixer.toml
fixer config validate
```

`fixer.toml` and `.env*` are ignored by Git. Keep the template public and keep
the effective files private.

## Discovery and precedence

Fixer loads configuration in this order, from highest to lowest precedence:

1. CLI overrides: `--offline`, `--proxy`, and `--local-root`.
2. Variables already present in the process environment. `RUST_LOG` specifically
   overrides both `[logging].filter` and `FIXER_LOGGING__FILTER`.
3. Variables loaded from `./.env`, only when the process environment does not
   already define the same key.
4. The selected configuration file.
5. Built-in defaults.

The selected file is:

1. `--config FILE` for the CLI;
2. `FIXER_CONFIG` for either binary;
3. `./fixer.toml` otherwise.

An explicitly selected missing file is an error. A missing default
`./fixer.toml` is allowed, so commands that need only defaults or environment
values can still run. Relative `FIXER_CONFIG` and `.env` lookup are based on the
process current working directory.

Relative `local_root`, `server.database`, `server.media_roots`, and
`server.web_root` paths are resolved against the selected configuration file's
directory. Existing local and media roots are canonicalized during load.

```bash
# Explicit CLI selection
fixer --config /etc/fixer/fixer.toml config validate

# Shared selection for CLI and server
FIXER_CONFIG=/etc/fixer/fixer.toml fixer config validate
FIXER_CONFIG=/etc/fixer/fixer.toml fixer-server
```

## TOML schema

`fixer.toml.example` is the maintained complete example. A compact shared file
looks like this:

```toml
offline = false
local_root = "/srv/media"
preferred_locales = ["zh-Hans", "zh-Hant", "ja", "en", "und"]
timeout_seconds = 30
auto_accept_confidence = 0.9
review_confidence = 0.6
output_preset = "full"
placement = "in_place"
conflict_policy = "review"
enabled_providers = ["local", "tmdb", "bangumi", "musicbrainz", "openlibrary"]

[providers.tmdb]
base_url = "https://api.themoviedb.org/3"
api_token_env = "TMDB_API_TOKEN"

[providers.bangumi]
base_url = "https://api.bgm.tv"

[providers.anilist]
base_url = "https://graphql.anilist.co"
access_token_env = "ANILIST_ACCESS_TOKEN"

[providers.musicbrainz]
base_url = "https://musicbrainz.org/ws/2"

[providers.openlibrary]
base_url = "https://openlibrary.org"
cover_base_url = "https://covers.openlibrary.org/b/"

[server]
bind = "127.0.0.1:3000"
database = "fixer.sqlite3"
media_roots = ["/srv/media"]
web_root = "web/dist"
allowed_origins = ["http://127.0.0.1:3000"]
https_termination = false
worker_count = 2

[server.trusted_proxy]
ranges = []
header = "x-forwarded-for"

[logging]
filter = "fixer_server=info,tower_http=info"
format = "pretty"
```

Unknown fields, malformed types, invalid endpoints, and invalid enum values fail
closed. The shared file may contain CLI-only and server-only fields; each binary
reads the complete schema.

## Environment encoding

Config-rs maps top-level fields to `FIXER_<FIELD>` and nested fields to
`FIXER_<SECTION>__<FIELD>`. Nested separators are two underscores. Lists use
comma-separated values.

| TOML field | Canonical environment variable | Default |
| --- | --- | --- |
| `offline` | `FIXER_OFFLINE` | `false` |
| `proxy` | `FIXER_PROXY` | unset |
| `local_root` | `FIXER_LOCAL_ROOT` | unset |
| `preferred_locales` | `FIXER_PREFERRED_LOCALES` | `zh-Hans,zh-Hant,ja,en,und` |
| `timeout_seconds` | `FIXER_TIMEOUT_SECONDS` | `30` |
| `auto_accept_confidence` | `FIXER_AUTO_ACCEPT_CONFIDENCE` | `0.9` |
| `review_confidence` | `FIXER_REVIEW_CONFIDENCE` | `0.6` |
| `output_preset` | `FIXER_OUTPUT_PRESET` | `full` |
| `placement` | `FIXER_PLACEMENT` | `in_place` |
| `conflict_policy` | `FIXER_CONFLICT_POLICY` | `review` |
| `enabled_providers` | `FIXER_ENABLED_PROVIDERS` | default provider set |
| `providers.tmdb.base_url` | `FIXER_PROVIDERS__TMDB__BASE_URL` | TMDB production API |
| `providers.bangumi.base_url` | `FIXER_PROVIDERS__BANGUMI__BASE_URL` | Bangumi production API |
| `providers.anilist.base_url` | `FIXER_PROVIDERS__ANILIST__BASE_URL` | AniList production API |
| `providers.musicbrainz.base_url` | `FIXER_PROVIDERS__MUSICBRAINZ__BASE_URL` | MusicBrainz production API |
| `providers.openlibrary.base_url` | `FIXER_PROVIDERS__OPENLIBRARY__BASE_URL` | Open Library production API |
| `providers.openlibrary.cover_base_url` | `FIXER_PROVIDERS__OPENLIBRARY__COVER_BASE_URL` | Open Library Covers API |
| `server.bind` | `FIXER_SERVER__BIND` | `127.0.0.1:3000` |
| `server.database` | `FIXER_SERVER__DATABASE` | `fixer.sqlite3` |
| `server.media_roots` | `FIXER_SERVER__MEDIA_ROOTS` | empty |
| `server.web_root` | `FIXER_SERVER__WEB_ROOT` | `web/dist` |
| `server.allowed_origins` | `FIXER_SERVER__ALLOWED_ORIGINS` | empty |
| `server.https_termination` | `FIXER_SERVER__HTTPS_TERMINATION` | `false` |
| `server.worker_count` | `FIXER_SERVER__WORKER_COUNT` | `2` |
| `server.trusted_proxy.ranges` | `FIXER_SERVER__TRUSTED_PROXY__RANGES` | empty |
| `server.trusted_proxy.header` | `FIXER_SERVER__TRUSTED_PROXY__HEADER` | `x-forwarded-for` |
| `logging.filter` | `FIXER_LOGGING__FILTER`, overridden by `RUST_LOG` | `fixer_server=info,tower_http=info` |
| `logging.format` | `FIXER_LOGGING__FORMAT` | `pretty` |

For example:

```dotenv
FIXER_OFFLINE=true
FIXER_ENABLED_PROVIDERS=local,bangumi,openlibrary
FIXER_SERVER__MEDIA_ROOTS=/srv/movies,/srv/music
FIXER_SERVER__ALLOWED_ORIGINS=https://fixer.example.com,http://127.0.0.1:5173
FIXER_LOGGING__FORMAT=json
RUST_LOG=fixer_server=debug,tower_http=info
```

Boolean environment values accept `1`, `true`, `yes`, `on`, `0`, `false`,
`no`, and `off`, without case sensitivity.

## General settings

| Field | Validation and behavior |
| --- | --- |
| `offline` | Skips providers that require network access. |
| `proxy` | Validated global HTTP proxy used by the default SDK transport. |
| `local_root` | Required by the current CLI `search` and `resolve` commands. |
| `preferred_locales` | Ordered, non-empty BCP 47 tags. |
| `timeout_seconds` | Positive integer passed to the default HTTP transport. |
| `auto_accept_confidence` | Finite `0.0..=1.0`, at least `review_confidence`. |
| `review_confidence` | Finite `0.0..=1.0`. |
| `output_preset` | `full` or `metadata`. |
| `placement` | `in_place`, `symlink`, `hardlink`, `copy`, or `reflink`. |
| `conflict_policy` | `prefer_first`, `review`, or `error`. |
| `enabled_providers` | Non-empty provider allowlist; duplicates are removed. |

The confidence thresholds are validated and reported but are not yet applied to
candidate selection. `metadata` drops writer-planned local-asset placement while
retaining metadata writes. Command-level `--placement` still overrides the
configured placement for one invocation.

Conflict behavior is:

| Value | Result when merge conflicts exist |
| --- | --- |
| `prefer_first` | Continue with deterministic provider precedence. |
| `review` | Return exit code `4` after emitting the plan; do not execute. |
| `error` | Return exit code `1` with a safe conflict count. |

## Providers and secrets

The default provider allowlist is `local`, `tmdb`, `bangumi`, `musicbrainz`, and
`openlibrary`. Add `anilist` explicitly to enable it. Registration and merge
precedence remain fixed regardless of list order.

Prefer runtime-only secrets:

```toml
[providers.tmdb]
api_token_env = "TMDB_API_TOKEN"

[providers.anilist]
access_token_env = "ANILIST_ACCESS_TOKEN"
```

```dotenv
TMDB_API_TOKEN=replace-with-a-real-secret
ANILIST_ACCESS_TOKEN=replace-with-a-real-secret
```

Reference names accept only uppercase ASCII letters, digits, and underscores.
The referenced variable must exist at load time. Direct `TMDB_API_TOKEN`,
`ANILIST_ACCESS_TOKEN`, `FIXER_API_KEY`, and canonical nested secret environment
overrides are resolved into redacted runtime-only fields and are not serialized
when Web settings are saved.

Credential precedence is:

- TMDB: `TMDB_API_TOKEN`, `FIXER_API_KEY`,
  `FIXER_PROVIDERS__TMDB__API_TOKEN`, referenced environment variable, then
  `providers.tmdb.api_token`.
- AniList: `ANILIST_ACCESS_TOKEN`,
  `FIXER_PROVIDERS__ANILIST__ACCESS_TOKEN`, referenced environment variable,
  then `providers.anilist.access_token`.

Direct `api_token` and `access_token` TOML values are supported but are plaintext
at rest. Do not commit them. Config debug output, CLI validation, and settings
responses redact values, but they cannot protect shell tracing or third-party
process inspection.

See [providers and connectivity](providers.md) for routing and endpoint details.

## Web settings persistence

The production server gives settings routes and workers the same `ConfigHandle`.
A successful `PUT /api/v1/settings` validates the complete candidate, writes the
selected TOML file atomically, and only then swaps the shared in-memory snapshot.
Each queued job takes a fresh snapshot before building its SDK flow, so the next
job observes the successful update without a restart.

On Unix, a newly persisted file is mode `0600`. Provider secret values are
write-only in the API: responses expose only configured booleans and optional
environment-reference names. A direct token entered in the Web UI is stored as
plaintext in the private TOML file; an environment reference remains a reference
and its resolved value is never written.

The Web settings surface edits workspace/provider fields, not `[server]` or
`[logging]`. Environment values still have higher precedence on the next process
start. Keep the configuration directory writable by the server account. A write
or validation failure leaves the previous file and in-memory snapshot active.

Legacy JSON files are read-only for this path. A server started with an explicit
JSON file can read it, but Web settings persistence returns an error rather than
writing TOML into a `.json` path.

## Legacy compatibility and migration

For one compatibility window, Fixer still accepts historical flat file fields,
provider endpoint variables, and single-underscore server variables. New
deployments should use the nested TOML schema and canonical double-underscore
environment names.

| Legacy file field | Canonical TOML field |
| --- | --- |
| `api_key` or `tmdb_api_token` | `providers.tmdb.api_token` |
| `tmdb_base_url` | `providers.tmdb.base_url` |
| `bangumi_base_url` | `providers.bangumi.base_url` |
| `musicbrainz_base_url` | `providers.musicbrainz.base_url` |
| `openlibrary_base_url` | `providers.openlibrary.base_url` |
| `openlibrary_cover_base_url` | `providers.openlibrary.cover_base_url` |
| `anilist_endpoint` | `providers.anilist.base_url` |
| `anilist_token` or `anilist_access_token` | `providers.anilist.access_token` |
| `secret_references.tmdb_api_token` | `providers.tmdb.api_token_env` |
| `secret_references.anilist_access_token` | `providers.anilist.access_token_env` |
| `anilist_enabled` | include or omit `anilist` in `enabled_providers` |

Legacy JSON is loaded only when selected explicitly:

```bash
fixer --config ./legacy.json config validate
```

To migrate:

1. Copy `fixer.toml.example` to a private `fixer.toml`.
2. Translate flat provider fields with the table above.
3. Move server settings under `[server]`, proxy settings under
   `[server.trusted_proxy]`, and tracing settings under `[logging]`.
4. Replace embedded tokens with `api_token_env` or `access_token_env` references.
5. Run `fixer --config ./fixer.toml config validate` in the same environment that
   will run the CLI or server.
6. Set `FIXER_CONFIG` for services whose working directory does not contain the
   migrated file.

Historical single-underscore server variables and endpoint variables such as
`TMDB_BASE_URL` remain accepted during the compatibility window, but canonical
nested variables should be used in new service definitions.

## Validation failures

Configuration load failures return CLI exit code `2`. Common causes include:

- malformed `.env` or TOML;
- an explicitly selected missing file;
- unknown fields or provider IDs;
- missing, non-directory, or inaccessible roots;
- malformed BCP 47 locale tags;
- zero timeout or worker count;
- confidence thresholds outside `0.0..=1.0`;
- invalid placement, conflict, output, or logging values;
- malformed or unset secret references;
- malformed provider endpoints, origins, CIDRs, or proxy URLs;
- incomplete trusted-proxy configuration.

Run `fixer config validate` with the same current directory, `FIXER_CONFIG`, and
environment as the failing process. The summary reports effective non-secret
policy and secret status without printing token values.

## Safety limit

Local EPUB scanning loads the EPUB archive into memory and parses only selected
container and OPF metadata entries. It rejects a selected metadata entry larger
than 1 MiB and reports a warning, but the whole archive currently has no separate
size cap.
