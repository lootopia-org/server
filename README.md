# Lootopia server

A **Rust backend** for Lootopia: a batteries-included authentication server plus
treasure-hunt gameplay, live events, and player profiles. Built to be easy to
read, run, and extend.

**Authentication** implements modern multi-factor auth:

- **Email + password** — PBKDF2-HMAC-SHA256 with a per-user **salt** (in the DB)
  and a server-wide **pepper** (in the environment, never in the DB).
- **Email verification** — a tokenized link is emailed (or printed to stdout in
  dev mode) and must be confirmed before login.
- **TOTP** (RFC 6238) — authenticator-app codes, enroll / verify / disable.
- **Passkeys / WebAuthn (FIDO2)** — register and log in with a security key,
  Touch ID, Windows Hello, etc.
- **MFA orchestration** — password is the first factor; TOTP is a second factor;
  a passkey is treated as strong (phishing-resistant) auth on its own.
- **JWT sessions** — signed tokens stored server-side so sessions can be revoked
  instantly (logout deletes the row).

**Gameplay** adds treasure hunts with ordered steps, participant tracking, and
player profiles (points, level, completed hunts).

**Live events** publish domain changes to **Kafka**, fan them out through
**Redis pub/sub**, and stream them to connected clients over **WebSockets**.

The stack: [axum](https://github.com/tokio-rs/axum) (HTTP + WebSockets),
[SQLx](https://github.com/launchbadge/sqlx) (async, compile-time-checked SQL +
migrations) on **PostgreSQL**, **Redis** (cache + pub/sub), **Kafka** (event
bus), [`pbkdf2`/`hmac`/`sha2`](https://github.com/RustCrypto) for crypto, and
[`webauthn-rs`](https://github.com/kanidm/webauthn-rs) for FIDO2.

---

## Quick start

```bash
# 1. Start infrastructure (or point env vars at your own instances)
docker compose up -d db redis kafka

# 2. Configure
cp .env.example .env
#   For real use, set strong secrets:
#     openssl rand -hex 32   # PASSWORD_PEPPER and JWT_SECRET

# 3. Apply database migrations (separate binary, run once / on schema change)
cargo run --bin migrate

# 4. Run the server
cargo run --bin server
# -> Listening on http://localhost:8080
```

Migrations are deliberately **not** run at server start-up, so booting extra
instances never races to alter tables. Re-run `cargo run --bin migrate` whenever
you change the schema in `migrations/`.

The server expects **PostgreSQL**, **Redis**, and **Kafka** to be reachable at
the URLs in `.env` (`DATABASE_URL`, `REDIS_URL`, `KAFKA_BROKERS`). The included
`docker-compose.yml` provides all three.

### Tests

Smoke tests drive the API with `curl` and cover auth, profiles, hunts, and
WebSockets. They need `python3`, `jq`, and (for the auth test) `oathtool` to
generate TOTP codes:

```bash
cargo run --bin migrate
cargo run --bin server > tests/server.log 2>&1 &   # log lets the test read the email link
tests/run_smoke.sh
```

`tests/run_smoke.sh` boots the server (in dev-email mode), runs four suites in
sequence, then tears down:

| Script | Coverage |
|---|---|
| `tests/smoke_user.sh` | register → verify → login → TOTP MFA → passkey options → logout |
| `tests/smoke_profile.sh` | create / get / update / delete profile, admin list |
| `tests/smoke_hunt.sh` | list / create / get / update / join / delete hunts |
| `tests/smoke_ws.sh` | WebSocket connect, subscribe, receive live events |

With no `SMTP_HOST` configured, verification emails are printed to the server's
stdout, so local development needs no mail server.

---

## Project layout

```
src/
  main.rs                 server entry point (the `server` binary)
  lib.rs                  crate root: wires the top-level modules together
  server.rs               boot: config -> pool -> webauthn -> kafka/redis -> axum
  routes.rs               top-level router: nests auth, profile, hunt, ws
  config.rs               configuration loaded from env / .env
  state.rs                AppState shared with every handler
  error.rs                ApiError -> HTTP response mapping
  bin/migrate.rs          standalone migration tool (the `migrate` binary)
  api/
    auth/                 registration, login, MFA, WebAuthn, sessions
      crypto/             password, TOTP, JWT, tokens
      handlers.rs         auth endpoint handlers
      router.rs           public + protected auth routes
      session.rs          JWT sessions + role-based extractors
      webauthn.rs         FIDO2 registration & authentication ceremonies
    hunts/                hunt CRUD, join/leave, participant listing
    hunt_steps/           step models + handlers (steps are created with hunts)
    profiles/             player profile CRUD + hunt-completion scoring
    ws/                   WebSocket live-event stream
    middleware/
      caching.rs          Redis response cache + invalidation on writes
      ownership.rs        hunt/step ownership checks for admin/partner routes
  event/
    event.rs              Event type + topic definitions
    event_handler.rs      Kafka publish, Redis pub/sub relay, in-process broadcast
  infra/
    kafka/                producer + consumer
    redis/                client, cache, pub/sub
  utils/                  DB pool, query macros, shared types
migrations/               SQL schema (embedded into the binary at build time)
tests/                    curl-based smoke tests
k8s/                      Kubernetes manifests (Kustomize)
```

`server.rs` and `routes.rs` live at the crate root — deliberately outside the
`api` module — so the HTTP wiring stays decoupled from the domain modules it
composes.

### How to add an endpoint

1. Add a handler in the relevant `api/<module>/handlers.rs`.
2. Register it on that module's router (e.g. `api/hunts/routes.rs`).
3. If it is a new top-level prefix, nest the router in `src/routes.rs`.
4. Protected routes take an `AuthedUser` (or `AuthedAdmin` / `AuthedPartner`)
   argument; the extractor enforces a fully-MFA'd session and the correct role.

---

## HTTP API

Tokens are returned in the JSON body. Send them back either as
`Authorization: Bearer <token>` **or** as a `session=<token>` cookie.

JSON field names use **camelCase**.

### Roles

Users have one of three roles: `admin`, `partner`, or `player`. Some endpoints
require a specific role (or admin-or-partner ownership of a resource).

### Public (auth)

| Method & path | Body | Purpose |
|---|---|---|
| `POST /auth/register` | `{username, email, password, bio, avatar}` | Create account, send verification email |
| `GET  /auth/verify-email?token=…` | – | Confirm email ownership |
| `POST /auth/resend-verification` | `{email}` | Re-send the verification link |
| `POST /auth/login` | `{email, password}` | First factor; returns `{token, mfaRequired, mfaMethods}` |
| `POST /auth/forgot-password` | `{email}` | Request a reset; emails a single-use link. Always returns a generic message (no account enumeration) |
| `POST /auth/reset-password` | `{token, new_password}` | Set a new password using the emailed token; single-use, expiring, and revokes existing sessions |
| `POST /auth/mfa/totp` | `{token, code}` | Second factor; elevates the session |
| `POST /auth/webauthn/login/begin` | `{email}` | Get assertion options for passkey login |
| `POST /auth/webauthn/login/complete` | `{handle, credential}` | Finish passkey login; returns `{token}` |

### Authenticated — auth (require a fully-MFA'd session)

| Method & path | Body | Purpose |
|---|---|---|
| `GET  /auth/me` | – | Current user info |
| `POST /auth/logout` | – | Revoke the current session |
| `POST /auth/totp/enroll/begin` | – | Create a secret; returns `{secret, otpauthUri}` |
| `POST /auth/totp/enroll/verify` | `{code}` | Confirm a code and enable TOTP |
| `POST /auth/totp/disable` | `{code}` | Disable TOTP |
| `POST /auth/webauthn/register/begin` | – | Get creation options for a new passkey |
| `POST /auth/webauthn/register/complete` | `{handle, credential}` | Store the new passkey |
| `GET  /auth/webauthn/credentials` | – | List registered passkeys |

### Profiles (authenticated)

| Method & path | Body | Role | Purpose |
|---|---|---|---|
| `GET  /profile` | – | player+ | Get the current user's profile |
| `POST /profile` | – | player+ | Create a profile (409 if one already exists) |
| `PATCH /profile` | `{huntId}` | player+ | Mark a joined hunt complete; awards points from hunt steps |
| `DELETE /profile` | – | player+ | Delete the current user's profile |
| `GET  /profile/list` | – | admin | List all profiles |

### Hunts (authenticated)

| Method & path | Body | Role | Purpose |
|---|---|---|---|
| `GET  /hunt` | – | player+ | List active hunts |
| `POST /hunt` | `{title, description, image, partnerId, difficulty, estimatedDuration, status?, steps[]}` | admin/partner | Create a hunt with ordered steps |
| `GET  /hunt/{id}` | – | player+ | Get a hunt and its steps |
| `PATCH /hunt/{id}` | `{title?, description?, …}` | admin or hunt owner | Update a hunt |
| `DELETE /hunt/{id}` | – | admin or hunt owner | Delete a hunt |
| `POST /hunt/join` | `{huntId}` | player+ | Join an active hunt |
| `POST /hunt/leave` | `{huntId}` | player+ | Leave a joined hunt |
| `GET  /hunt/joined` | – | player+ | List hunts the user has joined but not completed |

Hunt steps are created as part of `POST /hunt` and returned inline from
`GET /hunt/{id}`. Step types include `checkpoint` (location-based) and
riddle-style steps (answer-based).

### WebSocket — live events

| Path | Auth | Purpose |
|---|---|---|
| `GET /ws` | `session` cookie (fully MFA'd) | Stream live domain events |

On connect the server sends a `connected` control message. Clients start
subscribed to all topics (`*`). Send JSON control messages to filter:

```json
{"action": "subscribe",   "topics": ["hunts", "profiles"]}
{"action": "unsubscribe", "topics": ["profiles"]}
{"action": "ping"}
```

Event payloads mirror the Kafka topic structure:

| Topic | Event types |
|---|---|
| `hunts` | `created`, `updated`, `deleted`, `joined`, `leave` |
| `hunt_steps` | `complete`, `update`, `delete` |
| `profiles` | `updated` |

Subscribe to a specific resource with `hunts.<uuid>`.

### Login / MFA flow

```
POST /auth/login {email,password}
   ├─ password wrong            -> 401
   ├─ email not verified        -> 403 "email_not_verified"
   ├─ TOTP enabled              -> 200 {token, mfaRequired:true,  mfaMethods:["totp"]}
   │      then POST /auth/mfa/totp {token, code}  -> session elevated, reuse token
   └─ no second factor          -> 200 {token, mfaRequired:false, mfaMethods:[]}

Passkey login is a separate, self-sufficient path:
POST /auth/webauthn/login/begin {email}  ->  options + handle
POST /auth/webauthn/login/complete {handle, credential}  ->  {token} (fully authenticated)
```

The `credential` field in the WebAuthn `complete` calls is the JSON produced by
the browser's `navigator.credentials.create()` / `.get()`; the `begin` calls
return the matching `publicKey` options to feed into it.

### Forgot / reset password flow

Both endpoints are **public**, but resetting requires a token that is only ever
emailed to the address on file — a random caller cannot reset someone else's
password.

```
POST /auth/forgot-password {email}
   └─ always 200 "if an account exists, a reset link has been sent"
         (if the user exists, a single-use token is emailed as
          {PUBLIC_BASE_URL}/auth/reset-password?token=…, valid for
          PASSWORD_RESET_TTL_SECONDS)

POST /auth/reset-password {token, new_password}
   ├─ unknown / expired token   -> 400 "invalid or expired token"
   └─ valid token               -> 200; password updated, token consumed,
                                   and all existing sessions revoked
```

Reset tokens are stored **hashed (SHA-256)** in `password_reset_tokens`, so a
database leak does not yield usable reset links.

---

## Security notes

- **Pepper**: keep `PASSWORD_PEPPER` out of the database and out of version
  control. A DB-only breach is useless without it. Rotating it invalidates all
  existing password hashes (by design).
- **JWT secret**: keep `JWT_SECRET` out of version control. Rotating it
  invalidates all existing sessions.
- **PBKDF2 iterations**: defaults to 200k; raise it on faster hardware.
- **Sessions** are JWTs stored server-side, so they can be revoked instantly
  (logout deletes the row). A session created after the password step is marked
  `mfa_pending` and cannot reach protected routes until a second factor succeeds.
- **Password reset** is gated by a single-use, expiring token (hashed at rest)
  that is only delivered by email; `/auth/forgot-password` never reveals whether
  an account exists, and a successful reset revokes the user's existing sessions.
- **WebAuthn** challenges are single-use and time-limited (`CEREMONY_TTL_SECONDS`),
  and the signature counter is checked to detect cloned authenticators.
- **Role checks** gate admin and partner-only operations at the handler layer.
- For production, terminate TLS in front of the server (passkeys require a
  secure context and the cookie is flagged `Secure`).

---

## Requirements

- Rust 1.88+ (tested with 1.95) and Cargo.
- PostgreSQL 13+ (the included `docker-compose.yml` provides one).
- Redis 7+ and Kafka 3.9+ (also provided by `docker-compose.yml`).
- A system OpenSSL (`libssl-dev` / `openssl`) is needed at build time for the
  WebAuthn dependency.

  # TODO
  - [] add open search(adn a search endpoint)
  - [] add api for open tile map
