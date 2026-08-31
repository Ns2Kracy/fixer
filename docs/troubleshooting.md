# Troubleshooting

Start with the smallest diagnostic that preserves the failing state. Do not delete the SQLite database, overwrite metadata, or rerun a partially completed write until you understand the error and operation report.

## CLI exits nonzero

Fixer uses stable process status categories:

| Code | Meaning | First check |
| ---: | --- | --- |
| `1` | workflow/provider/planning/policy/execution failure | stderr and the exact input/output paths |
| `2` | invalid CLI input or configuration | `fixer config validate` and `<command> --help` |
| `3` | usable result with non-fatal warnings | structured warnings or stderr before approval |
| `4` | review required; conflicts blocked execution | provider precedence and `conflict_policy` |

Run:

```bash
fixer --help
fixer <command> --help
fixer --config ./fixer.json config validate
```

Global flags may appear before or after a subcommand, but Cargo development commands need the separator:

```bash
cargo run -p fixer-cli -- --offline scan ./library --kind movie --json
```

See [CLI workflows](cli.md) for JSON schema version `1` shapes and all flags.

## Configuration is invalid

`config validate` returns `2` for unknown fields/provider IDs, malformed locales/URLs/proxy values, zero timeouts, inconsistent confidence thresholds, empty provider lists, or unresolved secret references.

Check precedence before editing the file:

1. CLI flag
2. environment variable
3. selected JSON file
4. built-in default

`--config`, `FIXER_CONFIG`, and a local `./fixer.json` can select different files. The validation summary redacts tokens. It labels the effective source for `offline`, `proxy`, `local_root`, and the direct TMDB API key. The source printed beside AniList tracks the legacy `anilist_enabled` toggle, not `enabled_providers`, so it can say `default` even when an explicit provider allowlist enables AniList. Other effective values are printed without source provenance. See [configuration](configuration.md).

If a secret reference is configured, export the exact uppercase environment name in the same service/shell that starts Fixer. A variable in an interactive shell may not exist in systemd, launchd, a container, or an IDE task.

## No candidates in offline mode

Offline mode skips every network provider; it does not read a provider cache. Current `search` and `resolve` also require `--local-root` or `FIXER_LOCAL_ROOT`.

```bash
fixer --local-root /absolute/library --offline search movie 'Arrival'
```

Confirm that the root contains parseable sidecars and the title/year matches the local document. `offline_provider_skipped` is expected when network providers remain enabled. If no local provider can answer, disable offline mode or add a compatible network-free provider through the SDK.

## Provider request fails

Check in this order:

1. provider is enabled and compiled: `fixer providers list` plus `fixer config validate`;
2. TMDB token or optional AniList token resolves in the process environment;
3. endpoint override is a protocol-compatible API, not a website URL;
4. timeout is long enough for the route;
5. system or explicit proxy policy is the intended one;
6. provider service terms, quota, and availability permit the request.

To compare routing, test a bounded direct/system-proxy setup and an explicit Fixer proxy separately. Clearing environment variables does not force direct routing on macOS/Windows because Reqwest can inherit OS proxy settings. See [providers and connectivity](providers.md).

A provider failure can become exit `3` when local/other metadata remains usable. Read all warnings before writing.

## Plan differs from a previous preview

`fixer plan` and `fixer scrape --apply` are separate workflows. The apply command scans, resolves, and plans again; it does not execute a persisted plan from the prior invocation. Provider data, files, or configuration may have changed.

Use `scrape --dry-run` close to the planned write and keep inputs stable. On the server, review the current job's plan endpoint and fingerprint. Reject paths near the 2,048-character display limit because individual server path strings can be clipped without a per-field flag.

## Output target already exists

Default execution is no-overwrite. It refuses an existing file/symlink target rather than replacing metadata silently. An existing directory can satisfy `create_directory`.

Choose one safe response:

1. inspect and archive/remove the existing file outside Fixer, then generate a new plan;
2. move/rename the input or choose a different CLI placement so Fixer derives a different output root; SDK callers can provide a different output root/template directly;
3. in an SDK embedding only, deliberately select `OverwritePolicy::Replace` after accepting its platform-specific risk.

Do not loop `--apply`. A partial plan can leave completed earlier operations, and the next no-overwrite run will report them.

## Prepared plan is stale

`StalePlan` means a source or target changed after preparation. Discard the prepared plan, rescan/reresolve, preview the replacement, and approve again. Do not bypass fingerprints: they prevent a reviewed plan from mutating different filesystem state.

## Unsafe output target or outside media roots

Typical causes:

- `..`, absolute, or reserved path components in a template/plan target;
- a symlinked ancestor escaping the output root;
- an input/source/output outside `FIXER_SERVER_MEDIA_ROOTS`;
- a media root that does not exist or canonicalizes to a different path;
- relative non-symlink operation sources in an embedded server plan.

Use absolute canonical media roots. On macOS, `/var/...` can canonicalize to `/private/var/...`; compare `pwd -P` or the server's reported canonical root. Do not broaden the allowlist to `/` to hide a path error.

External server embedders must use absolute canonical copy/hardlink/reflink sources because current `FsPolicy` and SDK executor relative-source bases differ.

## Hardlink, symlink, or reflink fails

- **Hardlink:** source and target commonly need the same filesystem, and the filesystem must support hardlinks. Default no-overwrite publication also uses a hardlink for completed temporary files.
- **Symlink:** verify OS privileges, mount/server support, and the relative path from target parent to source. Moving one side can break the link.
- **Reflink:** the CLI requires real reflink support. It does not silently copy. SDK callers may opt into `FallbackToCopy` and should check for `CopiedFallback` storage cost.

Hardlinked media shares bytes through every path. Do not run tag/container mutation until all links are reviewed. See [placement semantics](output.md#placement-semantics).

## Server refuses to start

Run with explicit values and inspect stderr:

```bash
FIXER_SERVER_BIND='127.0.0.1:3000' \
FIXER_SERVER_PASSWORD='local-development-password' \
FIXER_SERVER_DATABASE='/absolute/state/fixer.sqlite3' \
FIXER_SERVER_MEDIA_ROOTS='/absolute/media' \
FIXER_SERVER_ALLOWED_ORIGINS='http://127.0.0.1:3000' \
cargo run -p fixer-server
```

Common failures:

- missing/empty/over-1024-byte password;
- missing media roots or a root that is absent/not a directory;
- non-loopback bind without authentication;
- malformed exact origin or trusted proxy CIDR/header;
- only one of the two trusted proxy variables is set;
- database parent is not writable;
- another process already owns the same database lease.

A missing `web/dist` does not block server startup; API health can pass while browser routes return 404. Build Web assets with `pnpm --dir web build`. Use `curl -i http://127.0.0.1:3000/api/v1/health` to separate API startup from static Web issues.

## Docker deployment fails

Start with the rendered configuration and current container state:

```bash
docker info
docker compose --env-file .env.docker config --quiet
docker compose --env-file .env.docker ps
docker compose --env-file .env.docker logs --tail=200 fixer
docker inspect --format '{{json .State.Health}}' \
  "$(docker compose --env-file .env.docker ps -q fixer)"
```

- **Docker daemon unavailable:** `docker info` must succeed. Start Docker Desktop or the host Docker service before pulling, building, or starting Fixer.
- **Missing required value:** Compose rejects empty `FIXER_SERVER_PASSWORD` or `FIXER_MEDIA_PATH`. Copy `.env.docker.example`, use a nonempty test or production password, and pass `--env-file .env.docker` to every Compose command.
- **Invalid media path:** `FIXER_MEDIA_PATH` must be absolute, must exist as a directory, and must be shared with Docker Desktop when applicable. Compose resolves an existing relative source against the project directory instead of rejecting it, so verify that the value starts with `/`. Resolve symlinks with `realpath` if the daemon reports a mount-source error.
- **Permission denied:** the process runs as UID 10001. Grant that UID search/read/write access appropriate to the host media tree, then verify with `docker compose --env-file .env.docker exec -T fixer test -w /media`. Also test `/data`; a failure there points to the named volume rather than the bind mount.
- **Unhealthy container:** inspect the health output and logs, then run `curl --fail http://127.0.0.1:3000/api/v1/health`. Startup configuration, SQLite, or media-root failures appear in the service log.
- **Port already allocated:** stop the conflicting listener or choose another `FIXER_PORT`. Update `FIXER_SERVER_ALLOWED_ORIGINS` to the same exact host port before recreating the service.

### Registry image cannot be pulled

Inspect the published manifest and pull the image selected by `FIXER_IMAGE`:

```bash
docker buildx imagetools inspect ghcr.io/ns2kracy/fixer:latest
docker buildx imagetools inspect ghcr.io/ns2kracy/fixer:edge
docker compose --env-file .env.docker pull
```

- **`manifest unknown` for `latest`:** no stable release tag exists before the first versioned release. To test published development builds, set `FIXER_IMAGE=ghcr.io/ns2kracy/fixer:edge` in `.env.docker`, then pull and start again.
- **`denied` or authentication error:** the GHCR package may still be private. A maintainer must complete the Public visibility step in [server deployment](server.md#maintainer-release-setup). Public-image users should not log in or store a GHCR token.
- **Unsupported architecture or incomplete manifest list:** the manifest should include `linux/amd64` and `linux/arm64`. Use `docker buildx imagetools inspect` to confirm both entries. Use a supported host or the [source-build override](server.md#source-build-deployment) if the host platform is absent.
- **Stale local tag:** run `docker compose pull` through the configured environment file, then recreate the service:

```bash
docker compose --env-file .env.docker pull
docker compose --env-file .env.docker up -d --wait
```

Do not use `docker compose down --volumes` while troubleshooting. It deletes the SQLite volume and removes the state needed to diagnose or recover jobs.

## Browser receives `cors_origin_denied`

Add the browser's exact scheme, host, and port to `FIXER_SERVER_ALLOWED_ORIGINS`. Do not include a trailing path or wildcard. Restart the server after changing environment configuration.

Direct cross-origin `PUT /api/v1/settings` is currently blocked because the server preflight method list omits `PUT`. Use the bundled same-origin Web app or the Vite development proxy until that server limitation changes.

Static Web files are outside API CORS middleware; loading HTML successfully does not prove API origin configuration is correct.

## Login succeeds but API calls fail

- `401 authentication_required`: the cookie/token is missing, expired, revoked, or scoped to another origin/path.
- `403 csrf_validation_failed`: a state-changing cookie request omitted/staled `X-CSRF-Token`.
- Secure cookie is never returned over HTTP: `FIXER_SERVER_HTTPS_TERMINATION=true` was set without HTTPS at the browser.
- Cookie lacks `Secure` behind HTTPS: set `FIXER_SERVER_HTTPS_TERMINATION=true` and restart.

Keep `credentials: "include"` for browser API calls. Store the CSRF value only in memory and refresh it by signing in again. Bearer API tokens do not use CSRF, but the current project has no token-management CLI/HTTP route.

## Job is interrupted after restart

Startup marks active jobs interrupted. Use retry only after inspecting the job and filesystem. A persisted execution reservation blocks automatic retry because the prior write may have started.

If an operation may have completed, compare the plan summary/fingerprint, execution counts, and files. The database does not store the full operation bytes/paths for later reconstruction. Generate a fresh no-write plan rather than deleting idempotency records.

Open event streams can delay graceful Ctrl-C. Close Web tabs/automation SSE clients before maintenance, then verify the process exited before copying SQLite.

## Database backup or restore behaves unexpectedly

Always stop the server before copying SQLite. Set an absolute database path so the restored process opens the intended file. Restore media roots at the same canonical paths or job inputs can become invalid.

Starting with a new password replaces the password hash but does not revoke restored unexpired sessions or API tokens. Use trusted backups. Session rows expire after their 12-hour lifetime; API tokens require revocation through an embedding that uses `SqliteJobStore`, or a fresh database when retaining old jobs/auth state is not required.

See [backup and restore](server.md#backup-and-restore).

## EPUB scan reports warnings or high memory use

The parser rejects a selected container/OPF entry larger than 1 MiB. It currently loads the whole EPUB archive into memory before applying those entry limits and has no whole-archive size cap. Treat unusually large/untrusted archives as unsafe input and constrain them before scanning.

`--update-epub` writes an intent file only; it does not modify the archive.

## Browser E2E times out or cannot launch

The local harness builds Web and Rust before Playwright, so cold runs can exceed a generic 60-second test-runner timeout. Run it directly or configure a longer outer timeout:

```bash
scripts/e2e-local.sh
```

If pinned Chromium is absent:

```bash
pnpm --dir web test:e2e:install
```

If Playwright Chromium cannot run but system Chrome is installed:

```bash
FIXER_E2E_BROWSER_CHANNEL=chrome scripts/e2e-local.sh
```

The harness prints the last server log lines on failure and cleans temporary state. Set `FIXER_E2E_PORT` only for debugging; a fixed occupied port disables automatic port retry.

## Collect safe diagnostics

Useful outputs:

```bash
fixer config validate
fixer providers list
cargo test -p <package> --test <target> -- --nocapture
pnpm --dir web test
RUST_BACKTRACE=1 cargo run -p fixer-server
```

Before sharing logs, remove provider tokens, bearer/session/CSRF values, proxy credentials, private origins, database paths, media paths, external IDs, and metadata content. Config debug/validation redact known secrets, but shell tracing, reverse proxies, and third-party tooling may not.
