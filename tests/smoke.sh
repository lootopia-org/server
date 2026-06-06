#!/usr/bin/env bash
#
# End-to-end smoke test for lootopia backend server, driven entirely over HTTP.
#
# Covers: registration, duplicate-registration, email verification, password
# login, /me, TOTP enrollment, the full MFA second-factor login, WebAuthn
# option generation, bad-credential handling, and logout/session revocation.
#
# Requirements: curl and python3 (used for JSON parsing and TOTP codes).
#
# Usage:
#   cargo run --bin migrate
#   cargo run --bin server > tests/server.log 2>&1 &
#   LOG_FILE=tests/server.log tests/smoke.sh
#
# Environment overrides:
#   BASE_URL   default http://localhost:8080
#   LOG_FILE   path to the server log; lets the test read the dev-mode email
#              verification link. If unset, email verification is skipped (and
#              login requires REQUIRE_VERIFIED_EMAIL=false on the server).
#   EMAIL      default: a unique address per run
#   PASSWORD   default: a sample passphrase

set -uo pipefail

BASE_URL="${BASE_URL:-http://localhost:8080}"
LOG_FILE="${LOG_FILE:-}"
PASSWORD="${PASSWORD:-correct horse battery staple}"
EMAIL="${EMAIL:-smoke+$(date +%s)-${RANDOM}@example.com}"

PASS=0
FAIL=0

c_green=$'\033[32m'; c_red=$'\033[31m'; c_dim=$'\033[2m'; c_off=$'\033[0m'

pass()  { PASS=$((PASS+1)); printf "  ${c_green}PASS${c_off} %s\n" "$1"; }
failf() { FAIL=$((FAIL+1)); printf "  ${c_red}FAIL${c_off} %s\n" "$1"; }
step()  { printf "\n${c_dim}== %s ==${c_off}\n" "$1"; }

# --- helpers ---------------------------------------------------------------

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1"; exit 2; }; }
need curl
need python3

# Extract a top-level JSON string/number field from stdin.
jget() { python3 -c 'import sys,json
try: d=json.load(sys.stdin)
except Exception: sys.exit(0)
v=d.get(sys.argv[1],"")
print(v if not isinstance(v,bool) else str(v).lower())' "$1"; }

# Compute the current TOTP code from a base32 secret.
totp() { python3 - "$1" <<'PY'
import sys, base64, hmac, hashlib, struct, time
key = base64.b32decode(sys.argv[1])
msg = struct.pack('>Q', int(time.time()) // 30)
h = hmac.new(key, msg, hashlib.sha1).digest()
o = h[19] & 0x0f
print('%06d' % ((struct.unpack('>I', h[o:o+4])[0] & 0x7fffffff) % 1000000))
PY
}

# request METHOD PATH [JSON_BODY] [BEARER]  -> sets $HTTP_STATUS and $BODY
HTTP_STATUS=""; BODY=""
request() {
  local method="$1" path="$2" body="${3:-}" bearer="${4:-}"
  local args=(-s -w $'\n%{http_code}' -X "$method" "$BASE_URL$path" -H 'Content-Type: application/json')
  [ -n "$bearer" ] && args+=(-H "Authorization: Bearer $bearer")
  [ -n "$body" ]   && args+=(-d "$body")
  local out; out=$(curl "${args[@]}")
  HTTP_STATUS="${out##*$'\n'}"
  BODY="${out%$'\n'*}"
}

expect_status() { # desc expected
  if [ "$HTTP_STATUS" = "$2" ]; then pass "$1 (HTTP $HTTP_STATUS)"
  else failf "$1: expected HTTP $2, got $HTTP_STATUS — $BODY"; fi
}
expect_contains() { # desc needle
  case "$BODY" in *"$2"*) pass "$1";; *) failf "$1: missing '$2' in: $BODY";; esac
}

# --- preflight -------------------------------------------------------------

step "preflight"
request GET /me
if [ "$HTTP_STATUS" = "000" ]; then
  echo "${c_red}Cannot reach $BASE_URL — is the server running?${c_off}"
  exit 2
fi
expect_status "server reachable, unauthenticated /me rejected" 401
echo "  using email: $EMAIL"

# --- registration ----------------------------------------------------------

step "registration"
request POST /auth/register "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
expect_status "register new user" 200
expect_contains "register acknowledged" "registered"

request POST /auth/register "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
expect_status "duplicate registration rejected" 409

# --- email verification ----------------------------------------------------

step "email verification"
VERIFIED=0
if [ -n "$LOG_FILE" ] && [ -f "$LOG_FILE" ]; then
  TOKEN=$(grep -A1 "verification link for $EMAIL" "$LOG_FILE" | grep -o 'token=[A-Za-z0-9_-]*' | tail -1 | sed 's/token=//')
  if [ -n "$TOKEN" ]; then
    request GET "/auth/verify-email?token=$TOKEN"
    expect_status "verify email with token" 200
    expect_contains "email marked verified" "verified"
    VERIFIED=1
  else
    failf "could not find verification token for $EMAIL in $LOG_FILE"
  fi
else
  echo "  ${c_dim}LOG_FILE not set — skipping email verification${c_off}"
fi

# --- password login --------------------------------------------------------

step "password login"
request POST /auth/login "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
if [ "$HTTP_STATUS" = "403" ] && [ "$VERIFIED" = "0" ]; then
  echo "${c_red}Login blocked: email not verified.${c_off}"
  echo "Run the server with REQUIRE_VERIFIED_EMAIL=false, or pass LOG_FILE=... so this test can verify the address."
  exit 2
fi
expect_status "login succeeds" 200
expect_contains "no MFA required yet" '"mfaRequired":false'
SESSION=$(printf '%s' "$BODY" | jget token)

request POST /auth/login "{\"email\":\"$EMAIL\",\"password\":\"wrong\"}"
expect_status "wrong password rejected" 401

step "authenticated /me"
request GET /me "" "$SESSION"
expect_status "/me with valid session" 200
expect_contains "/me returns our email" "$EMAIL"

# --- TOTP enrollment -------------------------------------------------------

step "TOTP enrollment"
request POST /auth/totp/enroll/begin "" "$SESSION"
expect_status "totp enroll begin" 200
SECRET=$(printf '%s' "$BODY" | jget secret)
[ -n "$SECRET" ] && pass "received TOTP secret" || failf "no TOTP secret returned"

CODE=$(totp "$SECRET")
request POST /auth/totp/enroll/verify "{\"code\":\"$CODE\"}" "$SESSION"
expect_status "totp enroll verify" 200
expect_contains "TOTP enabled" "TOTP enabled"

# --- MFA login flow --------------------------------------------------------

step "MFA login (password + TOTP)"
request POST /auth/login "{\"email\":\"$EMAIL\",\"password\":\"$PASSWORD\"}"
expect_status "login (mfa pending)" 200
expect_contains "MFA now required" '"mfaRequired":true'
expect_contains "TOTP offered as a method" "totp"
PENDING=$(printf '%s' "$BODY" | jget token)

request GET /me "" "$PENDING"
expect_status "pending session cannot access /me" 401

request POST /auth/mfa/totp "{\"token\":\"$PENDING\",\"code\":\"$(totp "$SECRET")\"}"
expect_status "submit TOTP second factor" 200

request GET /me "" "$PENDING"
expect_status "session elevated after MFA" 200
expect_contains "totp shown enabled" '"totpEnabled":true'

request POST /auth/mfa/totp "{\"token\":\"$PENDING\",\"code\":\"000000\"}"
expect_status "wrong TOTP code rejected" 401

# --- WebAuthn option generation -------------------------------------------

step "WebAuthn (passkeys)"
request POST /auth/webauthn/register/begin "" "$PENDING"
expect_status "passkey register options issued" 200
expect_contains "options contain a challenge" "challenge"
expect_contains "options contain publicKey block" "publicKey"

# A user with no registered passkey cannot start passkey login.
request POST /auth/webauthn/login/begin "{\"email\":\"$EMAIL\"}"
expect_status "passkey login refused without a passkey" 404

# --- logout ----------------------------------------------------------------

step "logout"
request POST /auth/logout "" "$PENDING"
expect_status "logout" 200
request GET /me "" "$PENDING"
expect_status "session revoked after logout" 401

# --- summary ---------------------------------------------------------------

printf "\n%s passed, %s failed\n" "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ]
