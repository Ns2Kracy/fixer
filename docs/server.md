# Server operations

`fixer-server` serves the versioned Axum API and the built Solid workspace from
one process. It uses the same `fixer.toml`, `.env`, provider registry, and SDK
runtime builder as the CLI. The production path runs the configured worker
count, stores jobs and authentication records in SQLite, and limits filesystem
access to configured media roots.

## Build and start

Build the Web application before starting a production-layout server:

```bash
pnpm --dir web install --frozen-lockfile
pnpm --dir web build
cp fixer.toml.example fixer.toml
```

Edit `[server]` so `media_roots` contains at least one existing directory and
`allowed_origins` contains the exact browser origin, then start:

```bash
cargo run -p fixer-server
```

For temporary overrides, use canonical nested environment variables:

```bash
FIXER_SERVER__MEDIA_ROOTS='/srv/media' \
FIXER_SERVER__ALLOWED_ORIGINS='http://127.0.0.1:3000' \
FIXER_LOGGING__FORMAT=json \
cargo run -p fixer-server
```

The loader reads `./.env` and `./fixer.toml` from the current directory unless
`FIXER_CONFIG` selects another file. Existing process variables override
`.env`; environment values override TOML. Relative database, media, and Web
paths are resolved against the selected file's directory.

The server validates configuration, initializes tracing, opens and migrates
SQLite, canonicalizes media roots, starts workers, binds the listener, and serves
`web/dist`. Startup fails before binding when required filesystem, network, or
logging settings are invalid. Authentication state belongs to SQLite: a database
without an administrator enables first-run registration, while an initialized
database keeps its existing credentials.

## Server and logging variables

TOML is the primary interface. Config-rs maps nested fields through two
underscores; comma-separated environment values map to TOML arrays.

| Variable | Default | Behavior |
| --- | --- | --- |
| `FIXER_CONFIG` | `./fixer.toml` | Explicit shared config path. A selected missing file is an error. |
| `FIXER_SERVER__BIND` | `127.0.0.1:3000` | Listener socket. Keep loopback unless a trusted proxy or private network controls access. |
| `FIXER_SERVER__DATABASE` | `fixer.sqlite3` | SQLite path, relative to the selected config directory unless absolute. |
| `FIXER_SERVER__MEDIA_ROOTS` | empty | Required comma-separated existing directories for production serving. |
| `FIXER_SERVER__WEB_ROOT` | `web/dist` | Static Web build directory, relative to the config directory unless absolute. |
| `FIXER_SERVER__ALLOWED_ORIGINS` | empty | Comma-separated exact browser origins; wildcards and URL paths are rejected. |
| `FIXER_SERVER__HTTPS_TERMINATION` | `false` | Adds `Secure` to cookies when the browser reaches Fixer over HTTPS; does not configure TLS. |
| `FIXER_SERVER__WORKER_COUNT` | `2` | Positive number of persistent job workers. |
| `FIXER_SERVER__TRUSTED_PROXY__RANGES` | empty | Direct-peer CIDRs allowed to provide the configured client-IP header. |
| `FIXER_SERVER__TRUSTED_PROXY__HEADER` | `x-forwarded-for` | One custom single-IP header; required when proxy ranges are set. |
| `FIXER_LOGGING__FILTER` | `fixer_server=info,tower_http=info` | Valid tracing-subscriber filter expression. |
| `RUST_LOG` | unset | Standard filter override; wins over TOML and `FIXER_LOGGING__FILTER`. |
| `FIXER_LOGGING__FORMAT` | `pretty` | `pretty` for local use or `json` for services and containers. |

Browser state-changing requests include `Origin`. Add every exact browser origin,
including scheme and non-default port. Requests carrying an unlisted origin fail
closed. Static Web files are outside API CORS middleware. Non-browser API
requests without `Origin` still require authentication.

Every HTTP response carries `x-request-id`. A valid inbound `x-request-id` of at
most 128 ASCII alphanumeric, `-`, `_`, `.`, or `:` characters is authoritative;
otherwise Fixer generates `req-` plus 32 lowercase hexadecimal characters. The
same ID is attached to the `http.request` tracing span, propagated in the
response header, and included in safe API error bodies. Use it to correlate a
browser failure with pretty or JSON logs.

See [configuration](configuration.md) for the full schema, precedence, secret
references, legacy aliases, and Web persistence semantics.

## Docker deployment

The registry deployment uses `ghcr.io/ns2kracy/fixer` and expects the GHCR package to have Public visibility. Public packages pull anonymously, so operators do not need `docker login` or registry credentials. The image packages `fixer-server`, `web/dist`, and the public TOML template. Its unprivileged entrypoint creates `/data/fixer.toml` with mode `0600` on first start. SQLite lives at `/data/fixer.sqlite3`, `/media` is the media root, and both CLI-compatible provider settings and Web changes use the generated TOML.

### Registry deployment

Download the two deployment files into a new directory. Set `FIXER_MEDIA_PATH` to the printed absolute directory and keep `FIXER_SERVER__ALLOWED_ORIGINS` equal to the exact URL you will open. Provider credentials belong in the ignored `.env.secrets` file. Compose requires the file, so the quick start creates it privately even when no credential is needed.

```bash
mkdir -p fixer-deployment
cd fixer-deployment
curl --fail --location --output compose.yaml \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/compose.yaml
curl --fail --location --output .env.docker.example \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/.env.docker.example
cp .env.docker.example .env.docker
mkdir -p media
printf 'media path: %s\n' "$PWD/media"
$EDITOR .env.docker
(umask 077 && : > .env.secrets)
# Optional: add TMDB_API_TOKEN or ANILIST_ACCESS_TOKEN.
$EDITOR .env.secrets
docker compose --env-file .env.docker config --quiet
docker compose --env-file .env.docker up -d --wait
```

Compose rejects a missing media-path value before startup and refuses to create a missing bind source. Docker Compose still resolves an existing relative bind source against the project directory, so verify that `FIXER_MEDIA_PATH` starts with `/`; relative media paths are outside this deployment contract. The generated config and SQLite database share the named `/data` volume. Environment values in `.env.docker` override the corresponding generated TOML fields on each start.

After Compose reports a healthy service, check the public health route and container health state:

```bash
curl --fail http://127.0.0.1:3000/api/v1/health
docker inspect --format '{{json .State.Health}}' \
  "$(docker compose --env-file .env.docker ps -q fixer)"
```

Open the exact URL in `FIXER_SERVER__ALLOWED_ORIGINS`. When the database has no registered administrator, the login page enables **Sign up**; this includes databases upgraded from the old startup-password release. Create the administrator before making the listener broadly reachable. Databases that already have an administrator require **Sign in** with the stored username and password.

The default port publishes only on `127.0.0.1`. Change `FIXER_PORT` and the allowed origin together when choosing another host port.

### Image channels and pinning

| Selector | Update policy |
| --- | --- |
| `latest` | Stable channel, updated by a version release. |
| `0.1.0`, `0.1`, `0` | Semantic-version release tags; use the full version when reproducibility matters. |
| `edge` | Development channel, updated from `main`. |
| `sha-<short-sha>` | Traceable tag for one published source commit. |
| `@sha256:<manifest-digest>` | Immutable multi-platform manifest. |

The template selects `ghcr.io/ns2kracy/fixer:latest`. To pin the image, replace its `FIXER_IMAGE` line with one of these values:

```dotenv
FIXER_IMAGE=ghcr.io/ns2kracy/fixer:0.1.0
# Or pin the manifest itself:
FIXER_IMAGE=ghcr.io/ns2kracy/fixer@sha256:<manifest-digest>
```

Tags can move according to the table; a digest does not.

### Registry upgrades and operation

Run `docker compose pull` with the deployment environment file before recreating a registry deployment. The explicit pull reports registry failures before Compose changes the running service.

```bash
docker compose --env-file .env.docker logs --tail=200 -f fixer
docker compose --env-file .env.docker stop
docker compose --env-file .env.docker up -d --wait

# Pull the selected FIXER_IMAGE, then recreate if it changed:
docker compose --env-file .env.docker pull
docker compose --env-file .env.docker up -d --wait

# Remove containers and networks but retain SQLite:
docker compose --env-file .env.docker down
```

Compose allows 30 seconds for graceful shutdown. Close Web/API event streams before maintenance so the server can drain connections and stop workers.

### Source-build deployment

A local source build requires a repository checkout. `compose.yaml` never builds an image by itself; add `compose.build.yaml` for every source-build Compose command.

```bash
git clone https://github.com/Ns2Kracy/fixer.git
cd fixer
cp .env.docker.example .env.docker
mkdir -p media
printf 'media path: %s\n' "$PWD/media"
$EDITOR .env.docker
(umask 077 && : > .env.secrets)
# Optional: add TMDB_API_TOKEN or ANILIST_ACCESS_TOKEN.
$EDITOR .env.secrets
docker compose \
  -f compose.yaml \
  -f compose.build.yaml \
  --env-file .env.docker \
  up --build -d --wait
```

Repeat both `-f` arguments when rebuilding after source changes. The override selects `fixer:local`; registry upgrades use the base file without the override.

### Storage and permissions

Compose mounts the `fixer-data` named volume at `/data`. Docker prefixes the volume name with the Compose project name. `docker compose down` preserves this volume, so `/data/fixer.toml` and `/data/fixer.sqlite3` survive service recreation and image upgrades. Web settings are therefore restart-stable and use the same provider configuration as workers.

`FIXER_MEDIA_PATH` is a writable bind mount at `/media`; Compose does not copy or manage that host directory. The image runs as UID and GID 10001. On Linux, grant UID 10001 search permission on parent directories and the read/write permissions required for the selected media tree. Use a dedicated group or ACL for shared libraries instead of broad world-write permissions. Check both mounts from a registry deployment with:

```bash
docker compose --env-file .env.docker exec -T fixer id
docker compose --env-file .env.docker exec -T fixer test -w /data
docker compose --env-file .env.docker exec -T fixer test -w /media
```

Source-build deployments can run the same checks with both Compose file arguments. The container root filesystem is read-only. `/tmp` is a 64 MiB tmpfs; only `/data` and the explicit `/media` bind persist writes.

`docker compose --env-file .env.docker down --volumes` deletes the named volume, including `fixer.toml` and the SQLite database. Use it only when you intend to destroy all persisted Fixer state and have a tested backup.

For a reverse proxy, keep `FIXER_BIND_IP=127.0.0.1`, set `FIXER_SERVER__ALLOWED_ORIGINS` to the public HTTPS origin, and set `FIXER_SERVER__HTTPS_TERMINATION=true`. The existing trusted-proxy rules in [Reverse proxy deployment](#reverse-proxy-deployment) still apply.

### Standalone Docker

A standalone container uses the registry image directly and needs the same exact origin, named SQLite volume, writable media bind, loopback port, and hardening controls. Replace the media source before running:

```bash
docker run --name fixer --detach --restart unless-stopped \
  --env-file .env.docker \
  --env-file .env.secrets \
  --publish 127.0.0.1:3000:3000 \
  --mount type=volume,source=fixer-data,target=/data \
  --mount type=bind,source=/absolute/path/to/media,target=/media \
  --read-only \
  --tmpfs /tmp:rw,size=64m,mode=1777 \
  --cap-drop ALL \
  --security-opt no-new-privileges:true \
  --user 10001:10001 \
  ghcr.io/ns2kracy/fixer:latest
```

Replace the final image reference with a semantic version or digest when pinning. Omit the second `--env-file` only when `.env.secrets` does not exist. The image supplies the config path, server bind, database, media root, Web root, JSON-capable tracing, health check, and `tini` defaults. Do not pass secrets as build arguments or place them in the image.

### Maintainer release setup

The first successful publish creates the GHCR package. Before announcing registry deployment, a maintainer must open the `Ns2Kracy/fixer` package settings, choose **Change visibility**, set the package to **Public**, and verify an anonymous pull. This one-time package setting enables the operator flow documented here; workflow `packages: write` permission does not make the package public.

## Reverse proxy deployment

Keep Axum on loopback when a reverse proxy terminates TLS:

```bash
FIXER_CONFIG='/etc/fixer/fixer.toml' \
FIXER_SERVER__BIND='127.0.0.1:3000' \
FIXER_SERVER__DATABASE='/var/lib/fixer/fixer.sqlite3' \
FIXER_SERVER__MEDIA_ROOTS='/srv/media' \
FIXER_SERVER__HTTPS_TERMINATION=true \
FIXER_SERVER__ALLOWED_ORIGINS='https://fixer.example.com' \
FIXER_SERVER__TRUSTED_PROXY__RANGES='127.0.0.1/32' \
FIXER_SERVER__TRUSTED_PROXY__HEADER='x-real-ip' \
FIXER_SERVER__WEB_ROOT='/opt/fixer/web/dist' \
FIXER_LOGGING__FORMAT=json \
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

- `GET /api/v1/health`, `GET /api/v1/auth/status`, and `POST /api/v1/auth/login` without an existing session;
- `POST /api/v1/auth/register` only while the database has no administrator account;
- authenticated provider, workspace, template, job, review, plan, execution, retry, cancellation, and event routes under `/api/v1`;
- hashed static assets under `/assets` with immutable caching;
- `index.html` and client routes with revalidation so new builds are picked up.

Configured cross-origin browsers can preflight `GET`, `HEAD`, and `POST`. The current preflight response omits `PUT`, so a frontend calling `PUT /api/v1/settings` directly from another origin is blocked by the browser. The bundled same-origin Web app and Vite development proxy do not require that cross-origin `PUT` preflight.

Job responses use `schema_version: 1`. Key routes include:

```text
GET  /api/v1/settings
PUT  /api/v1/settings
POST /api/v1/providers/{provider}/probe
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

The default database is `fixer.sqlite3` relative to the selected configuration directory. Set an absolute `server.database` or `FIXER_SERVER__DATABASE` in services when configuration and state must live in separate directories.

SQLite stores:

- jobs, progress, review decisions, plan counts/fingerprints, execution counts, and idempotency reservations;
- the single administrator username and Argon2id password hash in `fixer_users`;
- session token/CSRF digests and expiration times in `fixer_sessions`;
- API token names, digests, and revocation state.

The server acquires an exclusive process lease for the database identity. A second Fixer process cannot open the same database concurrently. Migrations run automatically at open.

Workspace settings changed through the Web UI are validated and atomically persisted to the selected `fixer.toml`; they are not stored in SQLite. The settings route and workers share one handle, and every queued job snapshots it before SDK construction, so the next job sees a successful update without a restart. Provider values returned by environment continue to have higher precedence after restart. Direct Web-entered provider tokens are write-only through the API but plaintext in the private TOML file; environment references persist only their variable names.

## Restart and interrupted jobs

Opening the store changes persisted active states (`scanning`, `searching`, `resolving`, `planning`, or `writing`) to `interrupted`. Workers seed queued jobs from SQLite after restart. Operators can retry an interrupted job through the job API or Web UI.

A job with a persisted execution reservation cannot be retried automatically. This fail-closed rule avoids duplicating a filesystem mutation when the process stopped after reserving or starting execution. Inspect the output plan, execution summary, and filesystem before deciding how to recover.

Ctrl-C first asks Axum to drain active HTTP connections, then requests worker shutdown. An open server-sent event connection can wait indefinitely for another event and delay that drain, so workers may not receive the shutdown signal promptly. Close Web/API event clients before maintenance. After Axum returns, workers stop at cooperative SDK stage boundaries and unfinished jobs become interrupted.

## Backup and restore

Use a stopped-server backup for a consistent, simple recovery point:

1. Close Web/API clients, especially job event streams; stop `fixer-server` and verify process exit.
2. Copy the configured SQLite database and selected `fixer.toml` to protected storage.
3. Record the service environment and any `.env`/secret injection separately; referenced secret values are intentionally absent from TOML.
4. Back up media and generated metadata through the library's normal backup process.

Do not back up the temporary process lease under the system temp directory. `web/dist` is reproducible from source and `web/pnpm-lock.yaml`; include it only when preserving a packaged release artifact.

Restore with the server stopped:

1. Restore `fixer.toml` and the SQLite copy with ownership limited to the server account; TOML must remain writable for Web settings.
2. Restore the referenced secret environment and remount the same media roots, then verify their canonical paths.
3. Restore the Web build or run `pnpm --dir web build`.
4. Start with the same `FIXER_CONFIG`. The restored administrator account, unexpired sessions, and unrevoked API tokens remain valid. Use a trusted backup, wait for sessions to expire, and use the embedding store API to revoke API tokens when required.
5. Verify `/api/v1/health`, sign in, inspect interrupted jobs, and confirm settings before queueing work.

The database contains library paths, job inputs, review decisions, plan summaries/fingerprints, the administrator username and password hash, and token/session digests. Full operation paths/bytes are not persisted in the plan summary. The TOML file may contain direct plaintext provider tokens entered through Web settings. Encrypt both backups and restrict access.

See [Security model](security.md) before exposing the service outside one trusted host.
