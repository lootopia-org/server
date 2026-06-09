#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
HUNT_ID="${HUNT_ID:-}"
USER_EMAIL="${USER_EMAIL:-test@example.com}"
USER_PASSWORD="${USER_PASSWORD:-password123}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@example.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"

PASS=0
FAIL=0

c_green=$'\033[32m'
c_red=$'\033[31m'
c_dim=$'\033[2m'
c_off=$'\033[0m'

pass() {
  PASS=$((PASS + 1))
  echo "  ${c_green}PASS${c_off} $1"
}
fail() {
  FAIL=$((FAIL + 1))
  echo "  ${c_red}FAIL${c_off} $1"
}

request() {
  local method="$1" path="$2" body="${3:-}" token="${4:-}"
  local args=(-s -w $'\n%{http_code}' -X "$method" "$BASE_URL$path"
    -H "Content-Type: application/json")
  [ -n "$token" ] && args+=(-H "Cookie: session=$token")
  [ -n "$body" ] && args+=(-d "$body")

  local out
  out=$(curl "${args[@]}")
  HTTP_STATUS="${out##*$'\n'}"
  BODY="${out%$'\n'*}"
}

expect() {
  local desc="$1" code="$2"
  if [ "$HTTP_STATUS" == "$code" ]; then
    pass "$desc"
  else fail "$desc (got $HTTP_STATUS, body=$BODY)"; fi
}

login() {
  local email="$1"
  local password="$2"
  local label="$3"

  request POST /auth/login "{\"email\":\"$email\",\"password\":\"$password\"}" >/dev/null
  expect "login succeeds ($label)" "200" >&2

  local token
  token=$(echo "$BODY" | jq -r '.token')

  if [ -z "$token" ] || [ "$token" = "null" ]; then
    echo "FAIL $label login: missing token" >&2
    echo "body: $BODY" >&2
    exit 1
  fi

  echo "$label token acquired" >&2

  echo "$token"
}

echo "== AUTH LOGIN =="

USER_TOKEN=$(login "$USER_EMAIL" "$USER_PASSWORD" "USER")
ADMIN_TOKEN=$(login "$ADMIN_EMAIL" "$ADMIN_PASSWORD" "ADMIN")

echo "user=$USER_TOKEN   admin=$ADMIN_TOKEN"

echo "== PROFILE SMOKE TEST =="

# --- create profile ---
request POST /profile "" "$USER_TOKEN"
expect "create profile (or conflict if exists)" "201"

# --- get profile ---
request GET /profile "" "$USER_TOKEN"
expect "get profile" "200"

# --- update profile (requires valid hunt_id) ---
if [ -n "$HUNT_ID" ]; then
  request PATCH /profile "{\"huntId\":\"$HUNT_ID\"}" "$USER_TOKEN"
  expect "update profile with hunt" "202"
else
  echo "  ${c_dim}SKIP update_profile (no HUNT_ID provided)${c_off}"
fi

# --- delete profile ---
request DELETE /profile "" "$USER_TOKEN"
if [ "$HTTP_STATUS" == "204" ] || [ "$HTTP_STATUS" == "404" ]; then
  pass "delete profile"
else
  fail "delete profile (got $HTTP_STATUS)"
fi

# --- list profiles (admin only) ---
if [ -n "$ADMIN_TOKEN" ]; then
  request GET /profile/list "" "$ADMIN_TOKEN"
  expect "list profiles (admin)" "200"
else
  echo "  ${c_dim}SKIP list_profiles (no ADMIN_TOKEN)${c_off}"
fi

echo
echo "$PASS passed, $FAIL failed"
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
