#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
set -- \
  "$ROOT_DIR/README.md" \
  "$ROOT_DIR/docs/configuration.md" \
  "$ROOT_DIR/docs/server.md" \
  "$ROOT_DIR/docs/development.md" \
  "$ROOT_DIR/docs/troubleshooting.md" \
  "$ROOT_DIR/docs/security.md" \
  "$ROOT_DIR/docs/providers.md" \
  "$ROOT_DIR/docs/cli.md"
TEMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/fixer-config-docs.XXXXXX")
trap 'rm -rf "$TEMP_DIR"' EXIT INT TERM

cp "$ROOT_DIR/fixer.toml.example" "$TEMP_DIR/fixer.toml"
(
  cd "$TEMP_DIR"
  home=${HOME:-$TEMP_DIR}
  env -i \
    HOME="$home" \
    PATH="$PATH" \
    CARGO_HOME="${CARGO_HOME:-$home/.cargo}" \
    RUSTUP_HOME="${RUSTUP_HOME:-$home/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}" \
    TMPDIR="${TMPDIR:-/tmp}" \
    cargo run --quiet --manifest-path "$ROOT_DIR/Cargo.toml" -p fixer-cli -- \
      --config "$TEMP_DIR/fixer.toml" config validate >/dev/null
)

if rg -n --pcre2 \
  'fixer\.json|FIXER_SERVER_(?!_)|FIXER_WEB_ROOT|live only in process memory|in-memory workspace settings|Workspace settings are not persisted' \
  "$@"
then
  printf 'stale public configuration guidance found\n' >&2
  exit 1
fi

for contract in \
  'fixer\.toml' \
  '\./\.env' \
  'FIXER_SERVER__BIND' \
  'FIXER_LOGGING__FORMAT' \
  'RUST_LOG' \
  'x-request-id'
do
  if ! rg -q "$contract" "$@"; then
    printf 'missing public configuration contract: %s\n' "$contract" >&2
    exit 1
  fi
done

rg -q 'FIXER_CONFIG: /data/fixer\.toml' "$ROOT_DIR/compose.yaml"
rg -q -- '- \.env\.secrets' "$ROOT_DIR/compose.yaml"
rg -q 'scripts/docker-entrypoint\.sh' "$ROOT_DIR/Dockerfile"
rg -q '^/fixer\.toml$' "$ROOT_DIR/.dockerignore"
rg -q 'umask 077.*\.env\.secrets' \
  "$ROOT_DIR/README.md" "$ROOT_DIR/docs/server.md"

printf 'configuration documentation contract is current\n'
