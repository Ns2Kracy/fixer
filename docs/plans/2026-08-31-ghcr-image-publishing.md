# GHCR Image Publishing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Publish public multi-platform Fixer images to GHCR and make the default Compose deployment runnable without a source checkout.

**Architecture:** `compose.yaml` becomes the registry-first public contract and `compose.build.yaml` restores explicit local builds. A pinned GitHub Actions workflow publishes `edge` from `main`, semantic version tags plus `latest` from `v*`, and OCI provenance for a combined amd64/arm64 manifest. Documentation separates anonymous registry deployment from source builds and records the one-time public-package step.

**Tech Stack:** Docker Compose, Docker Buildx/QEMU, GitHub Actions, GitHub Container Registry, Docker metadata/build actions, GitHub artifact attestations.

**Design:** `docs/plans/2026-08-31-ghcr-image-publishing-design.md`

---

### Task 1: Registry-first Compose contract

**Files:**

- Modify: `compose.yaml`
- Create: `compose.build.yaml`
- Modify: `.env.docker.example`

**Step 1: Capture the failing public/local Compose probes**

Run:

```bash
rg -n 'build:|image: fixer:local' compose.yaml
test ! -e compose.build.yaml
! rg -n '^FIXER_IMAGE=' .env.docker.example
```

Expected: the public file still requires a source build, the override is absent, and the image selector is undocumented.

**Step 2: Make `compose.yaml` pull the public stable image**

Replace the service build/image prefix with:

```yaml
services:
  fixer:
    image: "${FIXER_IMAGE:-ghcr.io/ns2kracy/fixer:latest}"
    pull_policy: always
```

Keep all existing ports, environment, volumes, read-only root, tmpfs, capability, security, and shutdown settings unchanged.

**Step 3: Add the local build override**

Create `compose.build.yaml`:

```yaml
services:
  fixer:
    build:
      context: .
    image: fixer:local
    pull_policy: build
```

**Step 4: Expose the image selector in the environment template**

Add this first line to `.env.docker.example`:

```dotenv
FIXER_IMAGE=ghcr.io/ns2kracy/fixer:latest
```

Do not add credentials. Public GHCR pulls must work without `docker login`.

**Step 5: Verify both rendered contracts**

Run:

```bash
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/media"
printf '%s\n' \
  'FIXER_SERVER_PASSWORD=test-only-password' \
  "FIXER_MEDIA_PATH=$tmp/media" \
  > "$tmp/docker.env"

docker compose --env-file "$tmp/docker.env" config > "$tmp/public.yaml"
docker compose \
  -f compose.yaml \
  -f compose.build.yaml \
  --env-file "$tmp/docker.env" \
  config > "$tmp/local.yaml"

rg -q 'image: ghcr.io/ns2kracy/fixer:latest' "$tmp/public.yaml"
rg -q 'pull_policy: always' "$tmp/public.yaml"
! rg -q 'build:' "$tmp/public.yaml"
rg -q 'image: fixer:local' "$tmp/local.yaml"
rg -q 'pull_policy: build' "$tmp/local.yaml"
rg -q 'context: ' "$tmp/local.yaml"
git diff --check -- compose.yaml compose.build.yaml .env.docker.example
```

Expected: the public contract has no source build; the merged local contract selects `fixer:local` and builds from the repository.

**Step 6: Perform a real local override build**

Run:

```bash
docker compose \
  -f compose.yaml \
  -f compose.build.yaml \
  --env-file "$tmp/docker.env" \
  build --pull

docker image inspect fixer:local --format '{{.Config.User}} {{.Config.StopSignal}}'
```

Expected: build exits zero and reports `10001:10001 SIGINT`.

**Step 7: Commit**

```bash
git add compose.yaml compose.build.yaml .env.docker.example
git commit -m "ops(compose): pull the public GHCR image by default"
```

### Task 2: Pinned multi-platform GHCR workflow

**Files:**

- Create: `.github/workflows/publish-container.yaml`

**Step 1: Verify the workflow is absent**

Run:

```bash
test ! -e .github/workflows/publish-container.yaml
```

Expected: PASS before implementation.

**Step 2: Add the publishing workflow**

Create `.github/workflows/publish-container.yaml`:

```yaml
name: Publish container image

on:
  push:
    branches:
      - main
    tags:
      - "v*"
  workflow_dispatch:

concurrency:
  group: publish-container-${{ github.ref }}
  cancel-in-progress: true

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ns2kracy/fixer

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
      attestations: write
      id-token: write
    steps:
      - name: Check out repository
        uses: actions/checkout@d23441a48e516b6c34aea4fa41551a30e30af803 # v6

      - name: Set up QEMU
        uses: docker/setup-qemu-action@c7c53464625b32c7a7e944ae62b3e17d2b600130 # v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@8d2750c68a42422c14e847fe6c8ac0403b4cbd6f # v3

      - name: Log in to GHCR
        uses: docker/login-action@c94ce9fb468520275223c153574b00df6fe4bcc9 # v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Generate image metadata
        id: meta
        uses: docker/metadata-action@c299e40c65443455700f0fdfc63efafe5b349051 # v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          flavor: |
            latest=false
          tags: |
            type=raw,value=edge,enable=${{ github.ref == 'refs/heads/main' }}
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=semver,pattern={{major}}
            type=raw,value=latest,enable=${{ startsWith(github.ref, 'refs/tags/v') }}
            type=sha,format=short
          labels: |
            org.opencontainers.image.source=https://github.com/${{ github.repository }}
            org.opencontainers.image.licenses=MIT OR Apache-2.0
            org.opencontainers.image.description=Local-first metadata scraper for movies, television, anime, music, and books

      - name: Build and push image
        id: push
        uses: docker/build-push-action@10e90e3645eae34f1e60eeb005ba3a3d33f178e8 # v6
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: true
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha,scope=fixer-container
          cache-to: type=gha,mode=max,scope=fixer-container

      - name: Attest image provenance
        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4
        with:
          subject-name: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          subject-digest: ${{ steps.push.outputs.digest }}
          push-to-registry: true
```

**Step 3: Validate workflow syntax and security contracts**

Run:

```bash
actionlint .github/workflows/publish-container.yaml
rg -n 'packages: write|attestations: write|id-token: write|linux/amd64,linux/arm64' \
  .github/workflows/publish-container.yaml
! rg -n 'PAT|DOCKER_PASSWORD|password: [^$]' .github/workflows/publish-container.yaml
```

If `actionlint` is not installed, install or run the current `rhysd/actionlint` release once; do not skip YAML/expression validation.

**Step 4: Confirm every action is pinned**

Run:

```bash
python3 - <<'PY'
from pathlib import Path
import re

path = Path('.github/workflows/publish-container.yaml')
text = path.read_text()
uses = re.findall(r'^\s*uses:\s*([^\s#]+)', text, re.MULTILINE)
assert len(uses) == 7, uses
for value in uses:
    ref = value.rsplit('@', 1)[1]
    assert re.fullmatch(r'[0-9a-f]{40}', ref), value
print('pinned-actions=7')
PY
```

Expected: `pinned-actions=7`.

**Step 5: Commit**

```bash
git add .github/workflows/publish-container.yaml
git commit -m "ci(ghcr): publish edge and versioned container images"
```

### Task 3: Copy-paste deployment documentation

**Files:**

- Modify: `README.md`
- Modify: `docs/server.md`
- Modify: `docs/troubleshooting.md`

**Step 1: Update the README quick start**

Replace the source-build quick start with a standalone flow that downloads `compose.yaml` and `.env.docker.example`, renames the latter to `.env.docker`, edits the password/media path, and runs:

```bash
docker compose --env-file .env.docker up -d --wait
```

State that `latest` is the stable channel, `edge` tracks `main`, and `FIXER_IMAGE` can pin `0.1.0` or a digest.

**Step 2: Split registry and source-build operations in `docs/server.md`**

Document:

- anonymous public pull from `ghcr.io/ns2kracy/fixer`;
- standalone download and startup;
- version pinning through `FIXER_IMAGE`;
- upgrades with `docker compose pull` and `up -d --wait`;
- local builds with both Compose files;
- tag policy (`edge`, semver, `latest`, SHA);
- the one-time GHCR Public visibility requirement;
- existing data/media/UID/hardening behavior remains unchanged.

Replace checkout-based `build --pull` upgrade instructions for registry users. Keep a separate local-build command using `compose.build.yaml`.

**Step 3: Add registry failures to `docs/troubleshooting.md`**

Cover:

- `manifest unknown` before the first stable tag;
- `denied`/authentication errors while the package is private;
- unsupported architecture or incomplete manifest list;
- stale local tags and `docker compose pull`;
- using `FIXER_IMAGE=ghcr.io/ns2kracy/fixer:edge` for development builds;
- inspecting manifests with `docker buildx imagetools inspect`.

Do not tell public-image users to store a GHCR token.

**Step 4: Verify documentation contracts and links**

Run:

```bash
rg -n 'ghcr.io/ns2kracy/fixer|compose.build.yaml|FIXER_IMAGE|docker compose pull|edge|latest' \
  README.md docs/server.md docs/troubleshooting.md

git diff --check -- README.md docs/server.md docs/troubleshooting.md
```

Run the repository's anchor-aware Markdown link check and expect no missing files or headings.

**Step 5: Commit**

```bash
git add README.md docs/server.md docs/troubleshooting.md
git commit -m "docs(ghcr): add copy-paste container deployment"
```

### Task 4: Local pre-publication acceptance

**Files:**

- Modify only if a concrete defect is found: `compose.yaml`, `compose.build.yaml`, `.env.docker.example`, `.github/workflows/publish-container.yaml`, `README.md`, `docs/server.md`, `docs/troubleshooting.md`

**Step 1: Start the local-build Compose stack**

Create a Docker Desktop-shared temporary media directory, random test password, free loopback port, and environment file. On this machine use a temporary directory below `/Users/ns2kracy/Coding` rather than `/var/folders` so UID 10001 sees the bind mount.

Run:

```bash
docker compose \
  --project-name fixer-ghcr-local \
  -f compose.yaml \
  -f compose.build.yaml \
  --env-file "$env_file" \
  up --build --detach --wait
```

Expected: the local override builds `fixer:local` and reaches healthy state without pulling GHCR.

**Step 2: Re-run runtime boundary probes**

Verify:

```bash
curl --fail "http://127.0.0.1:$port/api/v1/health"
docker compose ... exec -T fixer id -u
docker compose ... exec -T fixer test -w /data
docker compose ... exec -T fixer test -w /media
```

Expected: health status is `ok`, UID is `10001`, host media round-trip succeeds, root is read-only, and Compose stop exits zero through `SIGINT`.

**Step 3: Re-run SQLite persistence**

Write a marker to `/data`, record `/data/fixer.sqlite3` device/inode, force-recreate the service without deleting volumes, and verify the same SQLite identity plus marker remains.

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

Run Compose `down` without `--volumes`, remove only the temporary acceptance directory, and leave existing user Docker resources untouched.

**Step 6: Review and commit only fixes**

Use @code-review-and-quality and @verification-before-completion. Commit any concrete fix with a message naming the verified defect. Do not create an empty commit.

### Task 5: Publish and accept the `edge` channel

**Files:**

- No source changes expected.

**Step 1: Verify release authority and clean state**

Run:

```bash
gh auth status
git status --short --branch
git rev-parse HEAD
git tag --list 'v0.1.0'
git ls-remote --tags origin 'refs/tags/v0.1.0'
```

Expected: GitHub authentication is valid, `main` is clean, and `v0.1.0` does not exist locally or remotely.

**Step 2: Push `main`**

Run:

```bash
git push origin main
```

Expected: the push creates a `Publish container image` workflow run for the pushed HEAD.

**Step 3: Wait for the workflow**

Locate the run whose `headSha` equals the pushed HEAD, then run:

```bash
gh run watch "$run_id" --exit-status
```

Expected: checkout, QEMU, Buildx, GHCR login, metadata, multi-platform build/push, and attestation all succeed.

**Step 4: Verify the edge manifest**

Run:

```bash
docker buildx imagetools inspect ghcr.io/ns2kracy/fixer:edge
```

Expected: the manifest contains `linux/amd64` and `linux/arm64`, and OCI source points to `https://github.com/Ns2Kracy/fixer`.

**Step 5: Make the package Public**

Open the personal package settings for `Ns2Kracy/fixer`, select **Package settings** -> **Change visibility** -> **Public**, type `fixer`, and confirm. This is irreversible according to GitHub documentation and was explicitly approved in `docs/plans/2026-08-31-ghcr-image-publishing-design.md`.

Expected: the package page reports Public. Do not claim success until an anonymous pull works.

**Step 6: Prove anonymous edge pull and runtime behavior**

Use a fresh empty Docker config so cached GHCR credentials cannot affect the result:

```bash
anonymous_config=$(mktemp -d)
DOCKER_CONFIG="$anonymous_config" docker pull ghcr.io/ns2kracy/fixer:edge
```

Start a temporary registry-only Compose project with `FIXER_IMAGE=ghcr.io/ns2kracy/fixer:edge` and the same UID/media/SQLite acceptance probes as Task 4. Do not use `compose.build.yaml`.

Expected: anonymous pull succeeds, container reaches healthy, UID is `10001`, media round-trip works, and SQLite persists across recreation.

### Task 6: Publish and accept `v0.1.0`

**Files:**

- No source changes expected.

**Step 1: Reconfirm the release commit**

Run all Task 4 regression gates again on clean `main`, then record:

```bash
git rev-parse HEAD
git status --short --branch
```

Expected: clean `main`; HEAD matches the accepted `edge` image source revision.

**Step 2: Create and push the approved release tag**

Run:

```bash
git tag -a v0.1.0 -m "Release v0.1.0"
git push origin v0.1.0
```

Expected: the tag push starts a new container publishing workflow.

**Step 3: Wait for the tag workflow**

Locate the workflow run for `v0.1.0` and run:

```bash
gh run watch "$run_id" --exit-status
```

Expected: SUCCESS.

**Step 4: Verify stable tag equality**

Run:

```bash
for tag in latest 0.1.0 0.1 0
do
  docker buildx imagetools inspect --raw "ghcr.io/ns2kracy/fixer:$tag" \
    | shasum -a 256
done
```

Expected: all four manifest SHA-256 values are identical and each manifest contains amd64/arm64 images.

**Step 5: Run the literal copy-paste deployment**

In a new temporary directory, download `compose.yaml` and `.env.docker.example` from `raw.githubusercontent.com`, create an existing writable media directory, generate a random password, and run only:

```bash
docker compose --env-file .env.docker up -d --wait
```

Do not clone the repository and do not add `compose.build.yaml`.

Verify health, UID 10001, media host round-trip, read-only root, graceful stop exit zero, and SQLite persistence across force recreation.

**Step 6: Clean up and report**

Run Compose `down` without `--volumes`, remove only temporary acceptance directories and anonymous Docker config, and leave the named acceptance volume intact. Report:

- workflow run URLs and conclusions;
- public package URL;
- `edge` and stable manifest digests;
- supported platforms;
- copy-paste Compose acceptance evidence;
- the six implementation/release commits and `v0.1.0` tag SHA.
