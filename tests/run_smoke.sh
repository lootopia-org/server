#!/usr/bin/env bash
# Local helper: boots a throwaway server, runs the HTTP smoke test, tears down.
# Expects a reachable PostgreSQL via $DATABASE_URL (defaults to the dev DB).
set -u

PORT="${PORT:-8081}"
export DATABASE_URL="${DATABASE_URL:-host=localhost port=5433 dbname=authdb user=postgres password=postgres}"
export SMTP_HOST="${SMTP_HOST:-}"          # empty => dev-email mode (link to stdout)
export PORT
export ORIGIN="${ORIGIN:-http://localhost:$PORT}"
export PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-http://localhost:$PORT}"
export REQUIRE_VERIFIED_EMAIL="${REQUIRE_VERIFIED_EMAIL:-true}"
export RUST_LOG="${RUST_LOG:-info}"

BIN_DIR="${CARGO_TARGET_DIR:-target}/debug"
LOG_FILE="${LOG_FILE:-tests/server.log}"

pkill -f 'debug/server' 2>/dev/null || true
sleep 1

"$BIN_DIR/migrate" >/dev/null 2>&1 || { echo "migrate failed"; exit 2; }

"$BIN_DIR/server" > "$LOG_FILE" 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null' EXIT

for _ in $(seq 1 30); do
  code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:$PORT/me" || true)
  [ "$code" = "401" ] && break
  sleep 1
done

echo "===== smoke test (BASE_URL=http://localhost:$PORT) ====="
BASE_URL="http://localhost:$PORT" LOG_FILE="$LOG_FILE" bash tests/smoke.sh
exit $?
