-- Initial schema for the auth server.

CREATE TABLE IF NOT EXISTS users (
    id              BIGSERIAL PRIMARY KEY,
    email           TEXT        NOT NULL UNIQUE,
    email_verified  BOOLEAN     NOT NULL,
    password_salt   BYTEA       NOT NULL,
    password_hash   BYTEA       NOT NULL,
    user_handle     BYTEA       NOT NULL UNIQUE,
    totp_secret     BYTEA,
    totp_enabled    BOOLEAN     NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS email_tokens (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token       TEXT        NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id          BIGSERIAL PRIMARY KEY,
    user_id     BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token       TEXT        NOT NULL UNIQUE,
    mfa_pending BOOLEAN     NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS credentials (
    id            BIGSERIAL PRIMARY KEY,
    user_id       BIGINT      NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    credential_id BYTEA       NOT NULL UNIQUE,
    passkey       JSONB       NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS auth_ceremonies (
    id          BIGSERIAL PRIMARY KEY,
    handle      TEXT        NOT NULL UNIQUE,
    purpose     TEXT        NOT NULL,
    state       JSONB       NOT NULL,
    user_id     BIGINT      REFERENCES users (id) ON DELETE CASCADE,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_email_tokens_user_id ON email_tokens (user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_user_id ON sessions (user_id);
CREATE INDEX IF NOT EXISTS idx_credentials_user_id ON credentials (user_id);
