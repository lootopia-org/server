# server

A small, batteries-included **authentication server in Rust**, built to be easy
to read, run, and extend. It implements modern multi-factor authentication:

- **Email + password** — PBKDF2-HMAC-SHA256 with a per-user **salt** (in the DB)
  and a server-wide **pepper** (in the environment, never in the DB).
- **Email verification** — a tokenized link is emailed (or printed to stdout in
  dev mode) and must be confirmed before login.
- **TOTP** (RFC 6238) — authenticator-app codes, enroll / verify / disable.
- **Passkeys / WebAuthn (FIDO2)** — register and log in with a security key,
  Touch ID, Windows Hello, etc.
- **MFA orchestration** — password is the first factor; TOTP is a second factor;
  a passkey is treated as strong (phishing-resistant) auth on its own.

The stack: [axum](https://github.com/tokio-rs/axum) (HTTP),
[SQLx](https://github.com/launchbadge/sqlx) (async, compile-time-checked SQL +
migrations) on **PostgreSQL**, [`pbkdf2`/`hmac`/`sha2`](https://github.com/RustCrypto)
for crypto, and [`webauthn-rs`](https://github.com/kanidm/webauthn-rs) for FIDO2.

---

## Quick start

```bash
# 1. Start PostgreSQL (or use your own and edit DATABASE_URL)
docker compose up -d db

# 2. Configure
cp .env.example .env
#   For real use, set a strong PASSWORD_PEPPER (openssl rand -hex 32).

# 3. Apply database migrations (separate binary, run once / on schema change)
cargo run --bin migrate

# 4. Run the server
cargo run --bin server
# -> rust-auth-server listening on http://localhost:8080
```

Migrations are deliberately **not** run at server start-up, so booting extra
instances never races to alter tables. Re-run `cargo run --bin migrate` whenever
you change the schema in `migrations/`.

### Tests

`tests/smoke.sh` drives the whole API with `curl` (register → verify → login →
TOTP MFA → passkey options → logout). It needs `python3` (for JSON parsing and
generating TOTP codes):

```bash
cargo run --bin migrate
cargo run --bin server > tests/server.log 2>&1 &   # log lets the test read the email link
LOG_FILE=tests/server.log tests/smoke.sh
```

There is also a convenience wrapper, `tests/run_smoke.sh`, which boots the
server (in dev-email mode) and runs the smoke test against it, then tears down.

With no `SMTP_HOST` configured, verification emails are printed to the server's
stdout, so local development needs no mail server.

---

## Project layout

```
src/
  main.rs               server entry point (the `server` binary)
  lib.rs                crate root: wires the top-level modules together
  server.rs             boot: config -> pool -> webauthn -> axum (GLOBAL)
  routes.rs             the HTTP API: DTOs, handlers, the router (GLOBAL)
  bin/migrate.rs        standalone migration tool (the `migrate` binary)
  auth/
    config.rs           configuration loaded from env / .env
    state.rs            AppState shared with every handler (pool, config, webauthn)
    error.rs            ApiError -> HTTP response mapping
    models.rs           database row types
    db.rs               connection pool + migrations runner
    crypto/
      password.rs       PBKDF2 + salt + pepper
      totp.rs           RFC 6238 TOTP (+ RFC 4226 HOTP), Base32 secrets
      token.rs          CSPRNG URL-safe tokens
    email.rs            sending verification mail (dev mode = stdout)
    session.rs          session tokens + the `AuthedUser` request extractor
    webauthn.rs         FIDO2 registration & authentication ceremonies
migrations/             SQL schema (embedded into the binary at build time)
tests/smoke.sh          end-to-end curl test of the whole API
```

`server.rs` and `routes.rs` live at the crate root — deliberately outside the
`auth` module — so the HTTP wiring stays decoupled from the individual auth
building blocks it composes.

### How to add an endpoint

1. Add a request handler in `src/routes.rs`.
2. Register it on the router in `routes::router`. Protected routes simply take an
   `AuthedUser` argument and the extractor enforces a fully-MFA'd session.

---

## HTTP API

Tokens are returned in the JSON body. Send them back either as
`Authorization: Bearer <token>` **or** as a `session=<token>` cookie.

### Public

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

### Authenticated (require a fully-MFA'd session)

| Method & path | Body | Purpose |
|---|---|---|
| `GET  /me` | – | Current user info |
| `POST /auth/logout` | – | Revoke the current session |
| `POST /auth/totp/enroll/begin` | – | Create a secret; returns `{secret, otpauthUri}` |
| `POST /auth/totp/enroll/verify` | `{code}` | Confirm a code and enable TOTP |
| `POST /auth/totp/disable` | `{code}` | Disable TOTP |
| `POST /auth/webauthn/register/begin` | – | Get creation options for a new passkey |
| `POST /auth/webauthn/register/complete` | `{handle, credential}` | Store the new passkey |
| `GET  /auth/webauthn/credentials` | – | List registered passkeys |
| `GET  /profile` | – | Show a users profile |
| `POST  /profile` | - | create a new profile |
| `PATCH  /profile` | {hunt_id} | update a profile, in order to increase compleed hunts and points |
| `GET  /auth/webauthn/credentials` | – | List registered passkeys |





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
- **PBKDF2 iterations**: defaults to 200k; raise it on faster hardware.
- **Sessions** are opaque random tokens stored server-side, so they can be
  revoked instantly (logout deletes the row). A session created after the
  password step is marked `mfa_pending` and cannot reach protected routes until
  a second factor succeeds.
- **Password reset** is gated by a single-use, expiring token (hashed at rest)
  that is only delivered by email; `/auth/forgot-password` never reveals whether
  an account exists, and a successful reset revokes the user's existing sessions.
- **WebAuthn** challenges are single-use and time-limited (`CEREMONY_TTL_SECONDS`),
  and the signature counter is checked to detect cloned authenticators.
- For production, terminate TLS in front of the server (passkeys require a
  secure context and the cookie is flagged `Secure`).

---

## Requirements

- Rust 1.88+ (tested with 1.95) and Cargo.
- PostgreSQL 13+ (the included `docker-compose.yml` provides one).
- A system OpenSSL (`libssl-dev` / `openssl`) is needed at build time for the
  WebAuthn dependency.
