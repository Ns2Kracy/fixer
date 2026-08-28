# Docker Deployment Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a reproducible, hardened single-host Docker image and Compose deployment for the Fixer server and Web application.

**Architecture:** A Node build stage produces `web/dist`, a Rust build stage produces the locked release `fixer-server` binary, and a Debian slim runtime serves both as an unprivileged user. Compose supplies required secrets and media storage, persists SQLite, exposes loopback by default, and hardens the container without preventing approved media writes.

**Tech Stack:** Docker multi-stage builds, Docker Compose, Rust 1.85, Node 24.15, pnpm 11.22, Debian bookworm-slim, Axum health endpoint.

---

### Task 1: Reproducible application image

**Files:**

- Create: `Dockerfile`
- Create: `.dockerignore`
- Reference: `Cargo.toml`
- Reference: `Cargo.lock`
- Reference: `web/package.json`
- Reference: `web/pnpm-lock.yaml`
- Reference: `crates/fixer-server/src/main.rs`

**Step 1: Verify the image files are absent**

Run:

```bash
test ! -e Dockerfile
test ! -e .dockerignore
```

Expected: PASS before implementation.

**Step 2: Create `.dockerignore`**

Exclude Git data, Rust and Web host outputs, local dependency directories, SQLite files, browser reports, editor files, and all local environment files except `.env.docker.example`.

**Step 3: Create the multi-stage `Dockerfile`**

Use this runtime contract:

```dockerfile
FROM node:24.15.0-bookworm-slim AS web-build
WORKDIR /build/web
RUN corepack enable && corepack prepare pnpm@11.22.0 --activate
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

FROM rust:1.85-bookworm AS server-build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --locked --release -p fixer-server

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install --yes --no-install-recommends ca-certificates curl tini \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 fixer \
    && useradd --uid 10001 --gid fixer --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin fixer \
    && mkdir -p /app/web/dist /data /media \
    && chown -R fixer:fixer /data /media
WORKDIR /app
COPY --from=server-build /build/target/release/fixer-server /usr/local/bin/fixer-server
COPY --from=web-build /build/web/dist/ /app/web/dist/
ENV FIXER_SERVER_BIND=0.0.0.0:3000 \
    FIXER_SERVER_DATABASE=/data/fixer.sqlite3 \
    FIXER_SERVER_MEDIA_ROOTS=/media \
    FIXER_WEB_ROOT=/app/web/dist
EXPOSE 3000
VOLUME ["/data"]
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=5 \
    CMD ["curl", "--fail", "--silent", "--show-error", "http://127.0.0.1:3000/api/v1/health"]
USER 10001:10001
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["fixer-server"]
```

Do not use build arguments for secrets. Keep the media root out of `VOLUME` so Compose bind-mount behavior remains explicit.

**Step 4: Run static image checks**

Run:

```bash
git diff --check -- Dockerfile .dockerignore
rg -n 'pnpm install --frozen-lockfile|cargo build --locked --release -p fixer-server|USER 10001:10001|HEALTHCHECK' Dockerfile
```

Expected: all four build/runtime contracts are present and diff hygiene passes.

**Step 5: Build the image when the daemon is available**

Run:

```bash
docker build --pull --tag fixer:local .
```

Expected: both locked build stages complete and the runtime image is created. If Docker Desktop is unavailable, record the blocked runtime verification and continue with static checks; do not claim image success.

**Step 6: Commit**

```bash
git add Dockerfile .dockerignore
git commit -m "build(docker): add hardened application image"
```

### Task 2: Safe Compose deployment contract

**Files:**

- Create: `compose.yaml`
- Create: `.env.docker.example`

**Step 1: Write a failing Compose probe**

Run:

```bash
test ! -e compose.yaml
test ! -e .env.docker.example
```

Expected: PASS before implementation.

**Step 2: Add `.env.docker.example`**

Document these values without a real secret:

```dotenv
FIXER_SERVER_PASSWORD=replace-with-a-long-random-password
FIXER_MEDIA_PATH=/absolute/path/to/media
FIXER_BIND_IP=127.0.0.1
FIXER_PORT=3000
FIXER_SERVER_ALLOWED_ORIGINS=http://127.0.0.1:3000
FIXER_SERVER_HTTPS_TERMINATION=false
```

**Step 3: Add `compose.yaml`**

Implement one `fixer` service with:

```yaml
services:
  fixer:
    build:
      context: .
    image: fixer:local
    restart: unless-stopped
    ports:
      - "${FIXER_BIND_IP:-127.0.0.1}:${FIXER_PORT:-3000}:3000"
    environment:
      FIXER_SERVER_PASSWORD: "${FIXER_SERVER_PASSWORD:?set FIXER_SERVER_PASSWORD}"
      FIXER_SERVER_ALLOWED_ORIGINS: "${FIXER_SERVER_ALLOWED_ORIGINS:-http://127.0.0.1:3000}"
      FIXER_SERVER_HTTPS_TERMINATION: "${FIXER_SERVER_HTTPS_TERMINATION:-false}"
    volumes:
      - fixer-data:/data
      - type: bind
        source: "${FIXER_MEDIA_PATH:?set FIXER_MEDIA_PATH to an absolute existing directory}"
        target: /media
    read_only: true
    tmpfs:
      - /tmp:size=64m,mode=1777
    cap_drop:
      - ALL
    security_opt:
      - no-new-privileges:true
    stop_grace_period: 30s

volumes:
  fixer-data:
```

Do not duplicate image-default bind/database/media/Web-root values in Compose.

**Step 4: Verify required values fail closed**

Run with an explicit empty environment file so a repository `.env` cannot affect the result:

```bash
empty=$(mktemp)
if docker compose --env-file "$empty" config; then
  echo 'Compose unexpectedly accepted missing required variables' >&2
  exit 1
fi
rm -f "$empty"
```

Expected: interpolation reports missing `FIXER_SERVER_PASSWORD` or `FIXER_MEDIA_PATH`.

**Step 5: Verify rendered configuration**

Create a temporary environment file and existing media directory, then run:

```bash
tmp=$(mktemp -d)
mkdir "$tmp/media"
printf '%s\n' \
  'FIXER_SERVER_PASSWORD=test-only-password' \
  "FIXER_MEDIA_PATH=$tmp/media" \
  > "$tmp/docker.env"
docker compose --env-file "$tmp/docker.env" config > "$tmp/compose.rendered.yaml"
rg -n '127.0.0.1:3000|target: /media|read_only: true|no-new-privileges:true' "$tmp/compose.rendered.yaml"
rm -rf "$tmp"
```

Expected: Compose renders a loopback port, `/media` bind, read-only root, and privilege hardening.

**Step 6: Commit**

```bash
git add compose.yaml .env.docker.example
git commit -m "ops(docker): add secure single-host Compose deployment"
```

### Task 3: Docker operator documentation

**Files:**

- Modify: `README.md`
- Modify: `docs/server.md`
- Modify: `docs/troubleshooting.md`

**Step 1: Add a README quick start**

Add a short Docker section that:

1. copies `.env.docker.example` to `.env.docker`;
2. sets an absolute media path and random password;
3. runs `docker compose --env-file .env.docker up --build -d`;
4. waits for healthy status and opens the configured exact origin;
5. links to `docs/server.md#docker-deployment`.

**Step 2: Document the complete lifecycle**

Add `## Docker deployment` to `docs/server.md` covering:

- environment file setup and password generation;
- local startup and health inspection;
- `/data` named-volume persistence and `/media` writable bind mount;
- container UID 10001 host permission requirements;
- logs, stop, upgrade/rebuild, and non-destructive removal;
- destructive `docker compose down --volumes` warning;
- reverse proxy origin and HTTPS-termination overrides;
- standalone `docker run` requirements.

**Step 3: Add Docker failures to troubleshooting**

Document daemon unavailable, missing required interpolation values, non-absolute/nonexistent media paths, permission denied for UID 10001, unhealthy containers, port conflicts, and the health/log inspection commands.

**Step 4: Verify documentation contracts**

Run:

```bash
rg -n 'docker compose --env-file|UID 10001|down --volumes|FIXER_MEDIA_PATH' README.md docs/server.md docs/troubleshooting.md
```

Run the repository's anchor-aware Markdown link checker and expect no missing files or headings.

**Step 5: Commit**

```bash
git add README.md docs/server.md docs/troubleshooting.md
git commit -m "docs(docker): add deployment and recovery guide"
```

### Task 4: End-to-end container acceptance

**Files:**

- Modify only if verification finds a concrete defect: `Dockerfile`, `compose.yaml`, `.env.docker.example`, `README.md`, `docs/server.md`, `docs/troubleshooting.md`

**Step 1: Start an isolated Compose project**

Create a temporary media directory, environment file, project name, and free host port. Run:

```bash
docker compose --project-name fixer-acceptance --env-file "$env_file" up --build --detach --wait
```

Expected: service reaches healthy state.

**Step 2: Probe runtime boundaries**

Run:

```bash
curl --fail "http://127.0.0.1:$port/api/v1/health"
docker compose --project-name fixer-acceptance --env-file "$env_file" exec -T fixer id -u
docker compose --project-name fixer-acceptance --env-file "$env_file" exec -T fixer test -w /data
docker compose --project-name fixer-acceptance --env-file "$env_file" exec -T fixer test -w /media
```

Expected: health is `ok`, UID is `10001`, and both intended mounts are writable.

**Step 3: Verify persistence**

Record `/data/fixer.sqlite3`, recreate the service without deleting volumes, and confirm the same database remains. Never use `down --volumes` in the acceptance cleanup.

**Step 4: Run regression gates**

Run:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
pnpm --dir web test
pnpm --dir web build
git diff --check
```

Expected: every command exits zero.

**Step 5: Clean up safely**

Run Compose `down` without `--volumes`, remove only the temporary acceptance directory, and leave user Docker resources untouched.

**Step 6: Review and final commit if needed**

Use @code-review-and-quality and @verification-before-completion. If verification required fixes, commit only those fixes with a message naming the verified defect. Otherwise make no empty commit.
