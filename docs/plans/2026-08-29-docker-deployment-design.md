# Docker Deployment Design

**Date:** 2026-08-29
**Status:** Approved

## Goal

Provide a reproducible single-host Docker deployment for Fixer. One image builds and serves both the Web application and `fixer-server`; Docker Compose supplies the required password, persistent SQLite storage, an explicit media bind mount, and safe local defaults.

Registry publishing, Kubernetes manifests, automatic TLS, and bundled reverse-proxy configuration are outside this change.

## Approaches considered

1. **Dockerfile and Compose (selected).** Gives local operators a complete build/run path while keeping the image usable from other orchestrators.
2. **Dockerfile only.** Smaller maintenance surface, but every operator must recreate required environment, mounts, health checks, and security settings.
3. **Platform-specific deployment.** Kubernetes or Portainer configuration would add policy and infrastructure assumptions that the project does not currently make.

## Image architecture

The root `Dockerfile` uses three stages:

1. A pinned Node image enables the package's pinned pnpm version, installs from `web/pnpm-lock.yaml`, type-checks, and builds `web/dist`.
2. A pinned Rust image builds the `fixer-server` release binary with `Cargo.lock` and `--locked`.
3. A Debian slim runtime contains only the server binary, built Web assets, CA certificates, the health-probe client, and a minimal init process.

The runtime creates an unprivileged fixed-UID account, `/data`, `/media`, and `/app/web/dist`. It defaults to `0.0.0.0:3000` inside the container, stores SQLite at `/data/fixer.sqlite3`, uses `/media` as the allowed media root, and serves the copied Web build. The image health check calls `/api/v1/health` over loopback.

A root `.dockerignore` excludes Git state, host build output, dependencies, local databases, test artifacts, and environment files while retaining the committed Docker environment example.

## Compose contract

`compose.yaml` builds the local image and:

- requires `FIXER_SERVER_PASSWORD` and an absolute `FIXER_MEDIA_PATH`;
- publishes container port 3000 on `127.0.0.1` by default;
- stores SQLite in a named `/data` volume;
- bind-mounts the configured host media directory at `/media` with write access because approved jobs may mutate output;
- configures the exact browser origin and optional HTTPS-termination flag;
- uses a read-only root filesystem, writable `/tmp` tmpfs, dropped Linux capabilities, and `no-new-privileges`;
- restarts unless stopped and uses the image health check.

`.env.docker.example` documents non-secret defaults. Operators copy it to a local env file, replace the password and media path, then pass it explicitly with `docker compose --env-file` so the project does not depend on or overwrite an existing `.env`.

## Data and security boundaries

The database volume and host media directory are intentionally outside the container lifecycle. Image replacement must not remove them. Bind-mount permissions must allow container UID 10001 to write when jobs are applied.

The default host port is loopback-only. Public deployment remains the responsibility of an HTTPS reverse proxy. Operators must set the browser's exact public origin and `FIXER_SERVER_HTTPS_TERMINATION=true` behind HTTPS. Secrets are runtime environment values and are never copied into image layers.

The media mount is writable by design. Operators who only review plans may override it as read-only, but write execution will then fail closed.

## Failure behavior

Compose interpolation fails before startup when the password or media path is absent. Server startup then validates the password, existing media directory, database access, allowed origins, and bind address using the same production code path as a native deployment.

The health check distinguishes a running API from missing or broken static assets. Documentation covers port conflicts, bind-mount permissions, unhealthy containers, logs, database persistence, and upgrades.

## Verification

Acceptance requires:

1. `docker compose config` succeeds with an explicit test env file and fails without required values.
2. The image builds from the committed lockfiles.
3. The container runs as UID 10001 and reports healthy through `/api/v1/health`.
4. SQLite survives container recreation and `/media` resolves to the configured host directory.
5. Rust/Web regressions and documentation link checks remain green.
