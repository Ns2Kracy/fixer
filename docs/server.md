# Server operations

`fixer-server` serves the versioned Axum API and the built Solid workspace from one process. It runs two local workers, stores jobs and authentication records in SQLite, and limits filesystem access to configured media roots.

## Build and start

Build the Web application before starting a production-layout server:

```bash
pnpm --dir web install --frozen-lockfile
pnpm --dir web build

FIXER_SERVER_PASSWORD='replace-with-a-long-random-password' \
FIXER_SERVER_MEDIA_ROOTS='/srv/media' \
FIXER_SERVER_ALLOWED_ORIGINS='http://127.0.0.1:3000' \
cargo run -p fixer-server
```

The server validates configuration, opens and migrates SQLite, hashes the startup password, canonicalizes media roots, starts workers, binds the listener, and serves `web/dist`. Startup fails before binding when required security or filesystem settings are invalid.

## Environment variables

| Variable | Default | Behavior |
| --- | --- | --- |
| `FIXER_SERVER_BIND` | `127.0.0.1:3000` | Listener socket. Non-loopback values require a configured password; production serving requires a password for every bind. |
| `FIXER_SERVER_PASSWORD` | none | Required by `serve`; 1 to 1024 bytes. The value is Argon2id-hashed and never printed. |
| `FIXER_SERVER_DATABASE` | `fixer.sqlite3` | SQLite file path, relative to the process working directory unless absolute. |
| `FIXER_SERVER_MEDIA_ROOTS` | none | Required platform path-list of existing directories (`:` on Unix, `;` on Windows). Roots are canonicalized and deduplicated. |
| `FIXER_SERVER_HTTPS_TERMINATION` | `false` | Set exactly `true` when the browser reaches Fixer through HTTPS. Adds `Secure` to the session cookie; it does not configure TLS. |
| `FIXER_SERVER_ALLOWED_ORIGINS` | empty | Comma-separated exact browser origins allowed to send credentials. No wildcards, paths, query strings, fragments, or embedded credentials. |
| `FIXER_SERVER_TRUSTED_PROXY_RANGES` | disabled | Comma-separated CIDRs for direct socket peers allowed to supply the configured client-IP header. Must be set with `FIXER_SERVER_TRUSTED_PROXY_HEADER`. |
| `FIXER_SERVER_TRUSTED_PROXY_HEADER` | disabled | One custom client-IP header. Credential headers are rejected. Values must contain one IP address, not a forwarding chain. |
| `FIXER_WEB_ROOT` | `web/dist` | Static Web build directory. Use an absolute path for packaged deployments. |

Browser state-changing requests include an `Origin` header. Add every exact browser origin that should use the API, including the scheme and non-default port. API requests with an unlisted `Origin` fail closed. Static Web files are merged outside API CORS middleware and can still be served to a request carrying an unlisted origin. Non-browser API requests without `Origin` still require authentication.

`FIXER_SERVER_MEDIA_ROOTS` uses `std::env::split_paths`, not commas. Example with two Unix roots:

```bash
FIXER_SERVER_MEDIA_ROOTS='/srv/movies:/srv/music'
```

## Reverse proxy deployment

Keep Axum on loopback when a reverse proxy terminates TLS:

```bash
FIXER_SERVER_BIND='127.0.0.1:3000' \
FIXER_SERVER_PASSWORD='replace-with-a-long-random-password' \
FIXER_SERVER_DATABASE='/var/lib/fixer/fixer.sqlite3' \
FIXER_SERVER_MEDIA_ROOTS='/srv/media' \
FIXER_SERVER_HTTPS_TERMINATION=true \
FIXER_SERVER_ALLOWED_ORIGINS='https://fixer.example.com' \
FIXER_SERVER_TRUSTED_PROXY_RANGES='127.0.0.1/32' \
FIXER_SERVER_TRUSTED_PROXY_HEADER='x-real-ip' \
FIXER_WEB_ROOT='/opt/fixer/web/dist' \
fixer-server
```

The reverse proxy must:

- terminate HTTPS and redirect or reject cleartext traffic;
- remove any client-supplied `X-Real-IP`, then set one canonical IP value;
- prevent clients from reaching the Axum listener directly;
- preserve `Host`, cookies, `Authorization`, `X-CSRF-Token`, `Idempotency-Key`, and `Last-Event-ID` as needed;
- support long-lived server-sent event responses without buffering them indefinitely.

Trusted proxy configuration affects resolved client identity only. It is not an IP allowlist and Fixer currently does not apply request rate limiting based on `ClientIp`.

## Filesystem allowlist

Each configured media root must already exist and be a directory. `FsPolicy` stores canonical roots and applies them to job input, output, and operation sources:

- reads canonicalize the existing path and require it to remain under a root;
- writes make the path absolute, canonicalize the nearest existing ancestor, and reject escapes through symlinks;
- every output plan is checked again immediately before execution;
- absolute operation sources are canonicalized and required to remain under a root;
- SDK execution also rejects traversal, unsafe targets, stale fingerprints, and symlinked output ancestors.

Do not pass externally constructed server plans with relative non-symlink copy/hardlink/reflink sources. `FsPolicy` currently resolves those relative sources against the output root, while the SDK executor resolves them against the process working directory. Built-in production server writers currently emit metadata byte writes and no such relative source operation, but embedders must use absolute canonical sources until the two resolution rules are aligned.

Grant the server account only the OS permissions it needs. The allowlist cannot override operating-system permissions and does not protect other data when a configured root is too broad. Configure library directories, not `/`, a home directory, or a whole mounted host filesystem.

The workspace library browser skips symlinks during traversal and bounds directory/search results. Job execution can read or write only paths accepted by `FsPolicy`.

## Web and API routes

The production process serves:

- `GET /api/v1/health` and `POST /api/v1/auth/login` without an existing session;
- authenticated provider, workspace, template, job, review, plan, execution, retry, cancellation, and event routes under `/api/v1`;
- hashed static assets under `/assets` with immutable caching;
- `index.html` and client routes with revalidation so new builds are picked up.

Configured cross-origin browsers can preflight `GET`, `HEAD`, and `POST`. The current preflight response omits `PUT`, so a frontend calling `PUT /api/v1/settings` directly from another origin is blocked by the browser. The bundled same-origin Web app and Vite development proxy do not require that cross-origin `PUT` preflight.

Job responses use `schema_version: 1`. Key routes include:

```text
GET  /api/v1/jobs?limit=50&state=completed
POST /api/v1/jobs
GET  /api/v1/jobs/{id}
GET  /api/v1/jobs/{id}/review
POST /api/v1/jobs/{id}/review
GET  /api/v1/jobs/{id}/plan
POST /api/v1/jobs/{id}/execute
POST /api/v1/jobs/{id}/cancel
POST /api/v1/jobs/{id}/retry
GET  /api/v1/jobs/{id}/events
```

Execution requires a job created with `apply: true`, a review decision accepting every conflict index, `approved: true`, and an `Idempotency-Key` containing 1 to 256 visible ASCII characters. Reusing the same key and request fingerprint returns the existing reservation; a conflicting request fails. This prevents an ambiguous retry from scheduling a second write.

Lists accept 1 to 100 jobs. Review acceptance is capped at 4096 strictly increasing conflict indexes and must equal the complete zero-based conflict set. Candidate, warning, conflict, and operation collections expose truncation flags. Individual displayed text and path values are clipped to 2,048 characters without a per-field truncation flag, including `output_root`, source, and target. Do not approve a server plan whose relevant path could exceed that display bound; the API cannot prove its full suffix.

Axum JSON extractors also apply the framework default 2 MiB body limit because Fixer installs no explicit body-limit layer. Endpoint field limits remain stricter where documented.

## SQLite lifecycle

The default database is `./fixer.sqlite3` in the process working directory. Set an absolute `FIXER_SERVER_DATABASE` in services and containers so restarts do not select a different file.

SQLite stores:

- jobs, progress, review decisions, plan counts/fingerprints, execution counts, and idempotency reservations;
- the current password hash;
- session token/CSRF digests and expiration times;
- API token names, digests, and revocation state.

The server acquires an exclusive process lease for the database identity. A second Fixer process cannot open the same database concurrently. Migrations run automatically at open.

Workspace settings changed through the Web UI, including provider endpoints and provider tokens, currently live only in process memory. They reset to defaults on restart and are not part of the SQLite backup. Media roots and network settings come from environment variables.

## Restart and interrupted jobs

Opening the store changes persisted active states (`scanning`, `searching`, `resolving`, `planning`, or `writing`) to `interrupted`. Workers seed queued jobs from SQLite after restart. Operators can retry an interrupted job through the job API or Web UI.

A job with a persisted execution reservation cannot be retried automatically. This fail-closed rule avoids duplicating a filesystem mutation when the process stopped after reserving or starting execution. Inspect the output plan, execution summary, and filesystem before deciding how to recover.

Ctrl-C first asks Axum to drain active HTTP connections, then requests worker shutdown. An open server-sent event connection can wait indefinitely for another event and delay that drain, so workers may not receive the shutdown signal promptly. Close Web/API event clients before maintenance. After Axum returns, workers stop at cooperative SDK stage boundaries and unfinished jobs become interrupted.

## Backup and restore

Use a stopped-server backup for a consistent, simple recovery point:

1. Close Web/API clients, especially job event streams; stop `fixer-server` and verify process exit.
2. Copy the file named by `FIXER_SERVER_DATABASE` to protected storage.
3. Record deployment environment separately, especially media roots, origin, proxy, HTTPS, Web root, and current workspace settings that need manual recreation.
4. Back up media and generated metadata through the library's normal backup process.

Do not back up the temporary process lease under the system temp directory. `web/dist` is reproducible from source and `web/pnpm-lock.yaml`; include it only when preserving a packaged release artifact.

Restore with the server stopped:

1. Place the SQLite copy at the configured database path with ownership limited to the server account.
2. Restore or remount the same media roots and verify their canonical paths.
3. Restore the Web build or run `pnpm --dir web build`.
4. Start with the intended `FIXER_SERVER_PASSWORD`; startup replaces the stored password hash with this value. Existing unexpired sessions and unrevoked API tokens from the restored database remain valid because password rotation does not revoke them. The current operator surface cannot revoke all restored credentials; use a trusted backup, wait for sessions to expire, and use the embedding store API to revoke API tokens when required.
5. Verify `/api/v1/health`, sign in, inspect interrupted jobs, and recreate in-memory workspace settings.

The database contains library paths, job inputs, review decisions, plan summaries/fingerprints, password hashes, and token/session digests. Full operation paths/bytes are not persisted in the plan summary. Encrypt backups and restrict access even though plaintext passwords and tokens are not stored.

See [Security model](security.md) before exposing the service outside one trusted host.
