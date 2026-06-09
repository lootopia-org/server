#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
USER_EMAIL="${USER_EMAIL:-test@example.com}"
USER_PASSWORD="${USER_PASSWORD:-password123}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@example.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"
PARTNER_ID="${PARTNER_ID:-74a0c109-f29e-40a0-afd8-80903d21c040}"

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

echo "== HUNT SMOKE TEST =="

# --- list hunts (user) ---
request GET /hunt "" "$USER_TOKEN"
expect "list hunts" "200"

# --- create hunt (admin/partner) ---
CREATE_BODY=$(
  cat <<EOF
{
  "title": "Smoke Hunt",
  "description": "Test hunt",
  "image": null,
  "partnerId": "$PARTNER_ID",
  "difficulty": "easy",
  "estimatedDuration": 30,
  "steps": [
    {
      "stepOrder": 1,
      "title": "Step 1",
      "description": "First step",
      "type": "checkpoint",
      "latitude": "0",
      "longitude": "0",
      "points": 10
    }
  ]
}
EOF
)

request POST /hunt "$CREATE_BODY" "$ADMIN_TOKEN"

echo "$BODY"

if [ "$HTTP_STATUS" == "201" ]; then
  pass "create hunt"
  HUNT_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
else
  fail "create hunt"
  exit 1
fi

# --- get hunt ---
request GET "/hunt/$HUNT_ID" "" "$USER_TOKEN"
expect "get hunt by id" "200"

# --- update hunt ---
request PATCH "/hunt/$HUNT_ID" '{"title":"Updated Smoke Hunt", "status": "active"}' "$ADMIN_TOKEN"
expect "update hunt" "202"

# --- join hunt ---
request POST /hunt/join "{\"huntId\":\"$HUNT_ID\"}" "$USER_TOKEN"
expect "join hunt" "204"

# --- delete hunt ---
request DELETE "/hunt/$HUNT_ID" "" "$ADMIN_TOKEN"
if [ "$HTTP_STATUS" == "204" ] || [ "$HTTP_STATUS" == "404" ]; then
  pass "delete hunt"
else
  fail "delete hunt (got $HTTP_STATUS)"
fi

echo
echo "$PASS passed, $FAIL failed"
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
