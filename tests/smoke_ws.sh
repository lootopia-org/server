#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
WS_URL="${WS_URL:-ws://localhost:8080/ws}"
ADMIN_EMAIL="${ADMIN_EMAIL:-admin@example.com}"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin123}"
PARTNER_ID="${PARTNER_ID:-74a0c109-f29e-40a0-afd8-80903d21c040}"

PASS=0
FAIL=0
c_green=$'\033[32m'
c_red=$'\033[31m'
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
  local email="$1" password="$2" label="$3"
  request POST /auth/login "{\"email\":\"$email\",\"password\":\"$password\"}"
  expect "login succeeds ($label)" "200" >&2
  local token
  token=$(echo "$BODY" | jq -r '.token')
  if [ -z "$token" ] || [ "$token" = "null" ]; then
    echo "FAIL $label login: missing token" >&2
    exit 1
  fi
  echo "$label token acquired" >&2
  echo "$token"
}

if command -v websocat &>/dev/null; then
  WS_TOOL=websocat
elif command -v wscat &>/dev/null; then
  WS_TOOL=wscat
else
  echo "  ${c_red}SKIP${c_off} WebSocket tests: install websocat or wscat"
  WS_TOOL=none
fi

ws_collect() {
  local cookie="$1" topic="$2" outfile="$3" timeout="${4:-5}"
  local subscribe_msg
  subscribe_msg=$(printf '{"action":"subscribe","topics":["%s"]}' "$topic")

  if [ "$WS_TOOL" = websocat ]; then
    (
      echo "$subscribe_msg"
      sleep "$timeout"
    ) | timeout "$((timeout + 2))" websocat \
      --header "Cookie: session=$cookie" \
      -t \
      "$WS_URL" 2>/dev/null \
      >"$outfile" || true

  elif [ "$WS_TOOL" = wscat ]; then
    timeout "$((timeout + 2))" wscat \
      --connect "$WS_URL" \
      --header "Cookie: session=$cookie" \
      --execute "$subscribe_msg" \
      --wait "$timeout" 2>/dev/null \
      >"$outfile" || true
  fi
}

file_has_event_type() {
  local file="$1" event_type="$2"
  grep -q "\"eventType\"[[:space:]]*:[[:space:]]*\"$event_type\"" "$file" 2>/dev/null
}

echo "== AUTH LOGIN =="
ADMIN_TOKEN=$(login "$ADMIN_EMAIL" "$ADMIN_PASSWORD" "ADMIN")

echo
echo "== WS: CONNECTION =="

if [ "$WS_TOOL" = none ]; then
  echo "  (skipped — no WS client)"
else
  WELCOME_FILE=$(mktemp)
  ws_collect "$ADMIN_TOKEN" "hunts" "$WELCOME_FILE" 2

  if grep -q '"action"[[:space:]]*:[[:space:]]*"connected"' "$WELCOME_FILE"; then
    pass "ws welcome frame received"
  else
    fail "ws welcome frame received (got: $(cat "$WELCOME_FILE"))"
  fi

  if grep -q '"action"[[:space:]]*:[[:space:]]*"subscribed"' "$WELCOME_FILE"; then
    pass "ws subscribed ack received"
  else
    fail "ws subscribed ack received (got: $(cat "$WELCOME_FILE"))"
  fi

  rm -f "$WELCOME_FILE"
fi

echo
echo "== WS: PING/PONG =="

if [ "$WS_TOOL" = none ]; then
  echo "  (skipped)"
else
  PONG_FILE=$(mktemp)
  (
    echo '{"action":"ping"}'
    sleep 4
  ) | timeout 8 websocat \
    --header "Cookie: session=$ADMIN_TOKEN" \
    -t \
    "$WS_URL" 2>/dev/null \
    >"$PONG_FILE" || true

  if grep -q '"action"[[:space:]]*:[[:space:]]*"pong"' "$PONG_FILE"; then
    pass "ws pong received for ping"
  else
    fail "ws pong received for ping (got: $(cat "$PONG_FILE"))"
  fi

  rm -f "$PONG_FILE"
fi

echo
echo "== WS: ERROR HANDLING =="

if [ "$WS_TOOL" = none ]; then
  echo "  (skipped)"
else
  ERR_FILE=$(mktemp)
  (
    echo '{"action":"bogus","foo":"bar"}'
    sleep 2
  ) | timeout 4 websocat \
    --header "Cookie: session=$ADMIN_TOKEN" \
    -t \
    "$WS_URL" 2>/dev/null \
    >"$ERR_FILE" || true

  if grep -q '"action"[[:space:]]*:[[:space:]]*"error"' "$ERR_FILE"; then
    pass "ws error frame on invalid message"
  else
    fail "ws error frame on invalid message (got: $(cat "$ERR_FILE"))"
  fi

  rm -f "$ERR_FILE"
fi

echo
echo "== WS: AUTH GUARD =="

if [ "$WS_TOOL" = none ]; then
  echo "  (skipped)"
else
  HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Upgrade: websocket" \
    -H "Connection: Upgrade" \
    -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
    -H "Sec-WebSocket-Version: 13" \
    "$BASE_URL/ws" 2>/dev/null || true)

  if [ "$HTTP_CODE" = "401" ]; then
    pass "ws rejects connection without session cookie"
  else
    fail "ws rejects connection without session cookie (got HTTP $HTTP_CODE)"
  fi
fi

# ── Hunt CRUD + event assertions ───────────────────────────────────────────
echo
echo "== WS: HUNT EVENTS =="

CREATE_BODY=$(
  cat <<EOF
{
  "title": "WS Smoke Hunt",
  "description": "WebSocket event test",
  "image": null,
  "partnerId": "$PARTNER_ID",
  "difficulty": "easy",
  "estimatedDuration": 20,
  "steps": [
    {
      "stepOrder": 1,
      "title": "Step 1",
      "description": "Only step",
      "type": "checkpoint",
      "latitude": "0",
      "longitude": "0",
      "points": 5
    }
  ]
}
EOF
)

if [ "$WS_TOOL" = none ]; then
  echo "  (event assertions skipped — no WS client)"

  # Still run HTTP CRUD so the script is useful without a WS tool
  request POST /hunt "$CREATE_BODY" "$ADMIN_TOKEN"
  if [ "$HTTP_STATUS" = "201" ]; then
    pass "create hunt (HTTP only)"
    HUNT_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
    request PATCH "/hunt/$HUNT_ID" '{"title":"WS Updated","status":"active"}' "$ADMIN_TOKEN"
    expect "update hunt (HTTP only)" "202"
    request DELETE "/hunt/$HUNT_ID" "" "$ADMIN_TOKEN"
    expect "delete hunt (HTTP only)" "204"
  else
    fail "create hunt (HTTP only, got $HTTP_STATUS)"
  fi
else
  # ── helper: open WS listener in background, do HTTP op, capture events ──
  run_and_capture() {
    local label="$1" event_type="$2" outfile="$3"
    shift 3 # remaining args are the HTTP op: method path body token

    # Start background listener subscribed to "hunts" topic
    (
      echo '{"action":"subscribe","topics":["hunts"]}'
      sleep 6
    ) | timeout 8 websocat \
      --header "Cookie: session=$ADMIN_TOKEN" \
      -t \
      "$WS_URL" 2>/dev/null \
      >"$outfile" &
    local ws_pid=$!

    sleep 1 # give WS time to connect and subscribe

    # Execute HTTP operation
    request "$@"

    # Wait for listener to finish
    wait "$ws_pid" 2>/dev/null || true

    if file_has_event_type "$outfile" "$event_type"; then
      pass "ws event '$event_type' received after $label"
    else
      fail "ws event '$event_type' received after $label (events: $(cat "$outfile"))"
    fi
  }

  EV_CREATE=$(mktemp)
  EV_UPDATE=$(mktemp)
  EV_DELETE=$(mktemp)

  # --- create hunt + assert hunts.created event ---
  run_and_capture "create hunt" "hunts.created" "$EV_CREATE" \
    POST /hunt "$CREATE_BODY" "$ADMIN_TOKEN"

  if [ "$HTTP_STATUS" = "201" ]; then
    pass "create hunt HTTP 201"
    HUNT_ID=$(echo "$BODY" | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")
  else
    fail "create hunt HTTP (got $HTTP_STATUS)"
    HUNT_ID=""
  fi

  if [ -n "$HUNT_ID" ]; then
    # --- update hunt + assert hunts.updated event ---
    run_and_capture "update hunt" "hunts.updated" "$EV_UPDATE" \
      PATCH "/hunt/$HUNT_ID" '{"title":"WS Updated","status":"active"}' "$ADMIN_TOKEN"

    if [ "$HTTP_STATUS" = "202" ]; then
      pass "update hunt HTTP 202"
    else
      fail "update hunt HTTP (got $HTTP_STATUS)"
    fi

    # --- delete hunt + assert hunts.deleted event ---
    run_and_capture "delete hunt" "hunts.deleted" "$EV_DELETE" \
      DELETE "/hunt/$HUNT_ID" "" "$ADMIN_TOKEN"

    if [ "$HTTP_STATUS" = "204" ]; then
      pass "delete hunt HTTP 204"
    else
      fail "delete hunt HTTP (got $HTTP_STATUS)"
    fi
  fi

  rm -f "$EV_CREATE" "$EV_UPDATE" "$EV_DELETE"
fi

# ── WS: subscribe/unsubscribe topic management ─────────────────────────────
echo
echo "== WS: TOPIC MANAGEMENT =="

if [ "$WS_TOOL" = none ]; then
  echo "  (skipped)"
else
  TOPIC_FILE=$(mktemp)
  (
    echo '{"action":"subscribe","topics":["orders","payments"]}'
    sleep 1
    echo '{"action":"unsubscribe","topics":["payments"]}'
    sleep 1
  ) | timeout 5 websocat \
    --header "Cookie: session=$ADMIN_TOKEN" \
    -t \
    "$WS_URL" 2>/dev/null \
    >"$TOPIC_FILE" || true

  if grep -q '"action"[[:space:]]*:[[:space:]]*"subscribed"' "$TOPIC_FILE"; then
    pass "ws subscribed ack for additional topics"
  else
    fail "ws subscribed ack for additional topics (got: $(cat "$TOPIC_FILE"))"
  fi

  if grep -q '"action"[[:space:]]*:[[:space:]]*"unsubscribed"' "$TOPIC_FILE"; then
    pass "ws unsubscribed ack received"
  else
    fail "ws unsubscribed ack received (got: $(cat "$TOPIC_FILE"))"
  fi

  rm -f "$TOPIC_FILE"
fi

echo
echo "$PASS passed, $FAIL failed"
exit $([ "$FAIL" -eq 0 ] && echo 0 || echo 1)
