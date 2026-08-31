# GHCR Image Publishing Design

**Date:** 2026-08-31
**Status:** Approved

## Goal

Publish Fixer as a public multi-platform image at `ghcr.io/ns2kracy/fixer` and make the default Compose deployment usable without cloning or building the repository.

## Decisions

- Use GitHub Container Registry rather than Docker Hub.
- Publish development and release channels separately.
- Make `compose.yaml` registry-first and keep source builds in an explicit Compose override.
- Publish `linux/amd64` and `linux/arm64` images.
- Publish the initial stable image from Git tag `v0.1.0`.
- Make the GHCR package public after its first workflow publication so anonymous Compose pulls work.

## Public Compose Contract

`compose.yaml` remains the deployment entry point. It will:

- use `${FIXER_IMAGE:-ghcr.io/ns2kracy/fixer:latest}`;
- remove the source `build` section;
- pull the selected image from its registry;
- preserve the existing fail-closed password and media-path interpolation;
- preserve loopback publishing, the named SQLite volume, writable media bind, read-only root, tmpfs, dropped capabilities, no-new-privileges, health check, and graceful stop period.

The `FIXER_IMAGE` override lets operators pin a release or select the development channel without editing YAML:

```dotenv
FIXER_IMAGE=ghcr.io/ns2kracy/fixer:0.1.0
```

The default `latest` tag is convenient. Production operators should pin a version or digest when controlled upgrades matter.

## Local Build Contract

Add `compose.build.yaml` for source checkouts:

```yaml
services:
  fixer:
    build:
      context: .
    image: fixer:local
    pull_policy: build
```

Local source builds use both files:

```bash
docker compose \
  -f compose.yaml \
  -f compose.build.yaml \
  --env-file .env.docker \
  up --build -d --wait
```

This keeps public deployment independent of the repository while retaining the current development workflow.

## Publishing Workflow

Add `.github/workflows/publish-container.yaml` with these triggers:

- pushes to `main`;
- tags matching `v*`;
- manual `workflow_dispatch` for recovery.

The job will:

1. check out the exact revision;
2. configure QEMU and Docker Buildx;
3. authenticate to `ghcr.io` with the workflow `GITHUB_TOKEN`;
4. generate OCI labels and tags;
5. build and push `linux/amd64` and `linux/arm64` manifests;
6. publish a provenance attestation for the pushed digest.

Workflow permissions stay limited to:

```yaml
permissions:
  contents: read
  packages: write
  attestations: write
  id-token: write
```

Third-party and GitHub actions are pinned to complete commit SHAs. The workflow uses no personal access token or repository secret.

## Tag Matrix

| Event | Tags |
| --- | --- |
| Push to `main` | `edge`, `sha-<commit>` |
| Push tag `v0.1.0` | `0.1.0`, `0.1`, `0`, `latest`, `sha-<commit>` |
| Other `vX.Y.Z` tag | `X.Y.Z`, `X.Y`, `X`, `latest`, `sha-<commit>` |

`latest` changes only for version tags. A normal `main` push cannot replace the stable channel.

## Package Ownership And Visibility

Publishing through the repository workflow with `GITHUB_TOKEN` links the GHCR package to `Ns2Kracy/fixer`. GitHub creates a new package as private by default. After the first successful push, change `fixer` package visibility to Public before advertising the copy-paste deployment. Public GHCR images support anonymous pulls.

OCI metadata includes at least:

- `org.opencontainers.image.source=https://github.com/Ns2Kracy/fixer`;
- revision and version labels generated from the Git reference;
- the repository license and description.

## Operator Flow

The documented quick start downloads the two public deployment files, edits the private environment file, and starts the image:

```bash
mkdir fixer && cd fixer
curl --fail --remote-name \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/compose.yaml
curl --fail --output .env.docker \
  https://raw.githubusercontent.com/Ns2Kracy/fixer/main/.env.docker.example
$EDITOR .env.docker
docker compose --env-file .env.docker up -d --wait
```

Upgrade uses `docker compose pull` followed by `docker compose up -d --wait`. Existing `/data` and `/media` mounts remain unchanged.

## Failure Handling

- A build failure prevents every image push.
- A push or attestation failure fails the workflow.
- Missing `packages: write` permission fails during registry publication without exposing credentials.
- A private package causes anonymous pulls to fail; documentation calls out the one-time Public visibility step.
- Compose rejects missing required variables and nonexistent media bind sources as before.
- Local builds never publish unless a developer explicitly pushes an image outside this workflow.

## Verification

Before publication:

- validate workflow YAML and action pinning;
- render the registry Compose file with test-only values;
- render the local-build Compose merge and confirm `fixer:local` plus `pull_policy: build`;
- run repository Rust and Web regression gates;
- build the Docker image locally.

After pushing `main`:

- wait for the workflow to publish `edge`;
- inspect the GHCR manifest for `linux/amd64` and `linux/arm64`;
- make the package Public;
- pull `edge` anonymously and run the existing health, UID 10001, media mount, and SQLite persistence acceptance.

After pushing `v0.1.0`:

- verify all stable tags resolve to the same manifest digest;
- run the copy-paste Compose flow against `latest`;
- confirm health, graceful shutdown, UID 10001, media writes, and SQLite persistence without a source checkout.

## Sources

- GitHub Docs, Publishing Docker images: <https://docs.github.com/en/actions/use-cases-and-examples/publishing-packages/publishing-docker-images>
- GitHub Docs, Working with the Container registry: <https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-container-registry>
- Docker Docs, Compose service `image` and `pull_policy`: <https://docs.docker.com/reference/compose-file/services/#image>
