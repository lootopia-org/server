# haskell-auth-server

A small, batteries-included **authentication server in Haskell**, built to be easy
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

The stack: [Servant](https://www.servant.dev/) (type-safe HTTP),
[Persistent](https://www.yesodweb.com/book/persistent) (ORM + automatic
migrations) on **PostgreSQL**, [`crypton`](https://hackage.haskell.org/package/crypton)
for crypto, and [`webauthn`](https://hackage.haskell.org/package/webauthn) for FIDO2.

---

## Quick start

```bash
# 1. Start PostgreSQL (or use your own and edit DATABASE_URL)
docker compose up -d db

# 2. Configure
cp .env.example .env
#   For real use, set a strong PASSWORD_PEPPER (openssl rand -hex 32).

# 3. Apply database migrations (separate tool, run once / on schema change)
cabal run auth-migrate

# 4. Run the server
cabal run haskell-auth-server
# -> haskell-auth-server listening on http://localhost:8080
```

Migrations are deliberately **not** run at server start-up, so booting extra
instances never races to alter tables. Re-run `cabal run auth-migrate` whenever
you change `Auth/Models.hs`.

### Tests

`tests/smoke.sh` drives the whole API with `curl` (register → verify → login →
TOTP MFA → passkey options → logout). It needs `python3` (for JSON parsing and
generating TOTP codes):

```bash
cabal run auth-migrate
cabal run haskell-auth-server > tests/server.log 2>&1 &   # log lets the test read the email link
LOG_FILE=tests/server.log tests/smoke.sh
```

With no `SMTP_HOST` configured, verification emails are printed to the server's
stdout, so local development needs no mail server.

---

## Project layout

```
app/Main.hs              server entry point
tools/Migrate.hs         standalone migration tool (auth-migrate)
tests/smoke.sh           end-to-end curl test of the whole API
src/Auth/
  Config.hs              configuration loaded from env / .env
  Types.hs               AppEnv + the AppM monad (ReaderT AppEnv Handler), runDB
  Models.hs              the entire DB schema (Persistent) + migrateAll
  Database.hs            connection pool + run migrations
  Crypto/
    Password.hs          PBKDF2 + salt + pepper
    Totp.hs              RFC 6238 TOTP
    Token.hs             CSPRNG URL-safe tokens
    Base32.hs            tiny RFC 4648 Base32 (for TOTP secrets)
  Email.hs               sending verification mail (dev mode = stdout)
  Session.hs             session tokens + the AuthProtect handler
  Webauthn.hs            FIDO2 registration & authentication ceremonies
  Api.hs                 the Servant API type, JSON DTOs, handlers, WAI app
  Server.hs              boot: config -> pool -> migrate -> warp
```

### How to add an endpoint

1. Add a route to the `API` (or `ProtectedAPI`) type in `Auth/Api.hs`.
2. Add a handler of the matching type and wire it into `server` / `protectedServer`.

That's it — the type checker tells you if the handler doesn't match the route.
Protected routes get an `AuthedUser` automatically via `AuthProtect "session"`.

---

## HTTP API

Tokens are returned in the JSON body. Send them back either as
`Authorization: Bearer <token>` **or** as a `session=<token>` cookie.

### Public

| Method & path | Body | Purpose |
|---|---|---|
| `POST /auth/register` | `{email, password}` | Create account, send verification email |
| `GET  /auth/verify-email?token=…` | – | Confirm email ownership |
| `POST /auth/resend-verification` | `{email}` | Re-send the verification link |
| `POST /auth/login` | `{email, password}` | First factor; returns `{token, mfaRequired, mfaMethods}` |
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
[`webauthn-json`](https://github.com/github/webauthn-json)'s `create()` /
`get()` in the browser; the `begin` calls return the matching `publicKey`
options to feed into it.

---

## Security notes

- **Pepper**: keep `PASSWORD_PEPPER` out of the database and out of version
  control. A DB-only breach is useless without it. Rotating it invalidates all
  existing password hashes (by design).
- **PBKDF2 iterations**: defaults to 200k; raise it on faster hardware.
- **Sessions** are opaque random tokens stored server-side, so they can be
  revoked instantly (logout deletes the row). A session created after the
  password step is marked `mfaPending` and cannot reach protected routes until a
  second factor succeeds.
- **WebAuthn** challenges are single-use and time-limited (`CEREMONY_TTL_SECONDS`),
  and the signature counter is checked to detect cloned authenticators.
- For production, terminate TLS in front of the server (passkeys require a
  secure context and the cookie is flagged `Secure`).

---

## Requirements

- GHC 9.6.x and cabal (tested with GHC 9.6.7 / cabal 3.14).
- PostgreSQL 13+ (the included `docker-compose.yml` provides one).
