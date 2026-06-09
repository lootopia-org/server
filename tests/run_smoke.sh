#!/usr/bin/env bash
set -u

PORT="${PORT:-8080}"
export DATABASE_URL="${DATABASE_URL:-host=localhost port=5432 dbname=authdb user=postgres password=postgres}"
export SMTP_HOST="${SMTP_HOST:-}"
export PORT
export ORIGIN="${ORIGIN:-http://localhost:$PORT}"
export PUBLIC_BASE_URL="${PUBLIC_BASE_URL:-http://localhost:$PORT}"
export REQUIRE_VERIFIED_EMAIL="${REQUIRE_VERIFIED_EMAIL:-true}"
export RUST_LOG="${RUST_LOG:-info}"

BIN_DIR="${CARGO_TARGET_DIR:-target}/debug"
LOG_FILE="${LOG_FILE:-tests/server.log}"

pkill -f 'debug/server' 2>/dev/null || true
sleep 1

"$BIN_DIR/server" >"$LOG_FILE" 2>&1 &
SRV=$!
trap 'kill "$SRV" 2>/dev/null' EXIT

for _ in $(seq 1 30); do
  code=$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:$PORT/me" || true)
  [ "$code" = "401" ] && break
  sleep 1
done

echo "================================================================"
echo "===== smoke test auth (BASE_URL=http://localhost:$PORT) ====="
BASE_URL="http://localhost:$PORT" LOG_FILE="$LOG_FILE" bash tests/smoke_user.sh
echo "================================================================"

echo "================================================================"
echo "===== smoke test profile (BASE_URL=http://localhost:$PORT) ====="
BASE_URL="http://localhost:$PORT" LOG_FILE="$LOG_FILE" bash tests/smoke_profile.sh
echo "================================================================"

echo "================================================================"
echo "===== smoke test hunt (BASE_URL=http://localhost:$PORT) ====="
BASE_URL="http://localhost:$PORT" LOG_FILE="$LOG_FILE" bash tests/smoke_hunt.sh
echo "================================================================"
