ALTER TABLE users
    ADD COLUMN IF NOT EXISTS username   TEXT        UNIQUE,
    ADD COLUMN IF NOT EXISTS role       TEXT        CHECK (role IN ('admin', 'partner', 'player')),
    ADD COLUMN IF NOT EXISTS bio        TEXT,
    ADD COLUMN IF NOT EXISTS avatar              TEXT,
    ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ;

ALTER TABLE users
    DROP COLUMN IF EXISTS user_handle;

CREATE TABLE IF NOT EXISTS user_profiles (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             uuid        not null unique references users (id) on delete cascade,
    points              INTEGER     NOT NULL DEFAULT 0,
    level               REAL        NOT NULL DEFAULT 1.1,
    completed_hunts     INTEGER     NOT NULL DEFAULT 0,
    updated_at          TIMESTAMPTZ
);
CREATE TABLE hunts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title VARCHAR(255) NOT NULL,
  description TEXT,
  image VARCHAR(512),
  partner_id UUID NOT NULL REFERENCES partners(id),
  difficulty VARCHAR(10) CHECK (difficulty IN ('easy', 'medium', 'hard')),
  estimated_duration INT NOT NULL,
  latitude DECIMAL(10, 7) NOT NULL,
  longitude DECIMAL(10, 7) NOT NULL,
  status VARCHAR(20) DEFAULT 'draft'
    CHECK (status IN ('active', 'draft', 'archived')),
  launch_mode VARCHAR(10)
    CHECK (launch_mode IN ('draft', 'test', 'live')),
  rating DECIMAL(3, 2),
  created_at TIMESTAMPTZ DEFAULT NOW(),
  updated_at TIMESTAMPTZ DEFAULT NOW()
);
CREATE TABLE hunt_steps (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  hunt_id UUID NOT NULL REFERENCES hunts(id) ON DELETE CASCADE,
  step_order INT NOT NULL,
  title VARCHAR(255) NOT NULL,
  description TEXT,type VARCHAR(50),
  latitude DECIMAL(10, 7),
  longitude DECIMAL(10, 7),
  points: REAL,
  created_at TIMESTAMPTZ DEFAULT NOW(),
  UNIQUE (hunt_id, step_order)
);
CREATE TABLE IF NOT EXISTS hunts_participants (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL REFERENCES users(id)  ON DELETE CASCADE,
    hunt_id         UUID        NOT NULL REFERENCES hunts(id)  ON DELETE CASCADE,
    points_awarded  INTEGER     NOT NULL DEFAULT 0,
    joined_at       TIMESTAMPTZ, 
    completed_at    TIMESTAMPTZ,

    CONSTRAINT uq_completed_hunts_user_hunt UNIQUE (user_id, hunt_id)
);
CREATE TABLE IF NOT EXISTS badges (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT        NOT NULL UNIQUE,
    icon        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TABLE IF NOT EXISTS user_badges (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    badge_id    UUID        NOT NULL REFERENCES badges  (id) ON DELETE CASCADE,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, badge_id)
);
CREATE INDEX IF NOT EXISTS idx_user_profiles_user_id ON user_profiles (user_id);
CREATE INDEX IF NOT EXISTS idx_user_badges_user_id   ON user_badges   (user_id);
CREATE INDEX IF NOT EXISTS idx_user_badges_badge_id  ON user_badges   (badge_id);
CREATE INDEX IF NOT EXISTS idx_completed_hunts_user_id ON completed_hunts(user_id);
CREATE INDEX IF NOT EXISTS idx_completed_hunts_hunt_id ON completed_hunts(hunt_id);
