#!/bin/sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
PASSWORD=${FIXER_E2E_PASSWORD:-fixture-e2e-password}
HOST=127.0.0.1
SERVER_PID=
TEST_PID=
SERVER_LOG=
TEMP_DIR=

cleanup() {
  status=$?
  trap - EXIT
  if [ -n "$TEST_PID" ]; then
    kill "$TEST_PID" 2>/dev/null || true
    wait "$TEST_PID" 2>/dev/null || true
  fi
  if [ -n "$SERVER_PID" ]; then
    kill "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
  fi
  if [ "$status" -ne 0 ] && [ -n "$SERVER_LOG" ] && [ -f "$SERVER_LOG" ]; then
    printf '\nFixer E2E server log:\n' >&2
    tail -n 100 "$SERVER_LOG" >&2
  fi
  if [ -n "$TEMP_DIR" ]; then
    rm -rf "$TEMP_DIR"
  fi
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

TEMP_DIR=$(mktemp -d "${TMPDIR:-/tmp}/fixer-e2e.XXXXXX")
TEMP_DIR=$(CDPATH= cd -- "$TEMP_DIR" && pwd -P)
SERVER_LOG="$TEMP_DIR/server.log"
MEDIA_ROOT="$TEMP_DIR/library"
MOVIE_DIR="$MEDIA_ROOT/movie/In the Mood for Love (2000)"
MEDIA_PATH="$MOVIE_DIR"
OUTPUT_PATH="$MOVIE_DIR/movie.json"
DATABASE_PATH="$TEMP_DIR/fixer.sqlite"
FIXTURE_DIR="$ROOT_DIR/tests/fixtures/library/movie/In the Mood for Love (2000)"
CARGO_MESSAGES="$TEMP_DIR/cargo-build.json"

mkdir -p "$MEDIA_ROOT/movie"
cp -R "$FIXTURE_DIR" "$MEDIA_ROOT/movie/"

if [ -z "${FIXER_E2E_BROWSER_CHANNEL:-}" ]; then
  PLAYWRIGHT_BROWSER=$(pnpm --dir "$ROOT_DIR/web" exec node -e 'const { chromium } = require("@playwright/test"); process.stdout.write(chromium.executablePath())')
  if [ ! -x "$PLAYWRIGHT_BROWSER" ]; then
    printf 'Installing version-matched Playwright Chromium...\n'
    pnpm --dir "$ROOT_DIR/web" test:e2e:install
  fi
fi

printf 'Building Web production assets...\n'
pnpm --dir "$ROOT_DIR/web" build
printf 'Building Fixer server...\n'
cargo build \
  --locked \
  --manifest-path "$ROOT_DIR/Cargo.toml" \
  -p fixer-server \
  --message-format=json-render-diagnostics >"$CARGO_MESSAGES"
SERVER_BINARY=$(node -e '
  const fs = require("node:fs");
  let executable;
  for (const line of fs.readFileSync(process.argv[1], "utf8").split("\n")) {
    if (!line) continue;
    const message = JSON.parse(line);
    if (
      message.reason === "compiler-artifact" &&
      message.target.name === "fixer-server" &&
      message.target.kind.includes("bin") &&
      message.executable
    ) {
      executable = message.executable;
    }
  }
  if (!executable) {
    console.error("Cargo did not report the fixer-server executable");
    process.exit(1);
  }
  process.stdout.write(executable);
' "$CARGO_MESSAGES")

server_attempt=0
while :; do
  server_attempt=$((server_attempt + 1))
  PORT=${FIXER_E2E_PORT:-$(node -e 'const net = require("node:net"); const server = net.createServer(); server.listen(0, "127.0.0.1", () => { console.log(server.address().port); server.close(); });')}
  BASE_URL="http://$HOST:$PORT"
  : >"$SERVER_LOG"

  FIXER_SERVER_BIND="$HOST:$PORT" \
    FIXER_SERVER_PASSWORD="$PASSWORD" \
    FIXER_SERVER_DATABASE="$DATABASE_PATH" \
    FIXER_SERVER_MEDIA_ROOTS="$MEDIA_ROOT" \
    FIXER_SERVER_ALLOWED_ORIGINS="$BASE_URL" \
    FIXER_WEB_ROOT="$ROOT_DIR/web/dist" \
    "$SERVER_BINARY" >"$SERVER_LOG" 2>&1 &
  SERVER_PID=$!

  ready=false
  health_attempt=0
  while [ "$health_attempt" -lt 300 ]; do
    if curl --fail --silent --show-error "$BASE_URL/api/v1/health" >/dev/null 2>&1; then
      if kill -0 "$SERVER_PID" 2>/dev/null; then
        ready=true
      fi
      break
    fi
    if ! kill -0 "$SERVER_PID" 2>/dev/null; then
      break
    fi
    health_attempt=$((health_attempt + 1))
    sleep 0.1
  done

  if [ "$ready" = true ]; then
    break
  fi
  if kill -0 "$SERVER_PID" 2>/dev/null; then
    printf 'Timed out waiting for %s/api/v1/health.\n' "$BASE_URL" >&2
    exit 1
  fi
  wait "$SERVER_PID" 2>/dev/null || true
  SERVER_PID=
  if [ -n "${FIXER_E2E_PORT:-}" ] || [ "$server_attempt" -ge 5 ]; then
    printf 'Fixer server exited before becoming healthy.\n' >&2
    exit 1
  fi
  printf 'Retrying Fixer server on a new ephemeral port...\n' >&2
done

printf 'Running critical browser flow at %s...\n' "$BASE_URL"
FIXER_E2E_BASE_URL="$BASE_URL" \
  FIXER_E2E_PASSWORD="$PASSWORD" \
  FIXER_E2E_MEDIA_PATH="$MEDIA_PATH" \
  FIXER_E2E_OUTPUT_PATH="$OUTPUT_PATH" \
  pnpm --dir "$ROOT_DIR/web" test:e2e &
TEST_PID=$!
if wait "$TEST_PID"; then
  test_status=0
else
  test_status=$?
fi
TEST_PID=
exit "$test_status"
