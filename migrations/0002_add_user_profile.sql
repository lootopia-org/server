CREATE TABLE IF NOT EXISTS user_profiles (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL UNIQUE REFERENCES users (id) ON DELETE CASCADE,
    points          INTEGER NOT NULL DEFAULT 0,
    level           REAL NOT NULL DEFAULT 1.1,
    completed_hunts INTEGER NOT NULL DEFAULT 0,
    updated_at      TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS hunts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title VARCHAR(255) NOT NULL,
    description TEXT,
    image VARCHAR(512),
    partner_id UUID NOT NULL REFERENCES users(id),
    difficulty VARCHAR(10) CHECK (difficulty IN ('easy', 'medium', 'hard')),
    estimated_duration INT NOT NULL,
    status VARCHAR(20) DEFAULT 'draft'
        CHECK (status IN ('active', 'draft', 'archived')),
    rating DECIMAL(3, 2),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS hunt_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hunt_id UUID NOT NULL REFERENCES hunts(id) ON DELETE CASCADE,
    step_order INT NOT NULL,
    title VARCHAR(255) NOT NULL,
    description TEXT,
    type VARCHAR(50),
    awnser VARCHAR(50),
    latitude VARCHAR(255),
    longitude VARCHAR(255)
    points REAL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE (hunt_id, step_order)
);

CREATE TABLE IF NOT EXISTS hunt_participants (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    hunt_id         UUID NOT NULL REFERENCES hunts(id) ON DELETE CASCADE,
    points_awarded  INTEGER NOT NULL DEFAULT 0,
    joined_at       TIMESTAMPTZ,
    completed_at    TIMESTAMPTZ,
    CONSTRAINT uq_completed_hunts_user_hunt UNIQUE (user_id, hunt_id)
);

CREATE TABLE hunt_step_completions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    step_id      UUID NOT NULL REFERENCES hunt_steps(id) ON DELETE CASCADE,
    hunt_id      UUID NOT NULL REFERENCES hunts(id) ON DELETE CASCADE,
    completed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, step_id)
);

CREATE TABLE IF NOT EXISTS badges (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL UNIQUE,
    icon        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS user_badges (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    badge_id    UUID NOT NULL REFERENCES badges (id) ON DELETE CASCADE,
    unlocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, badge_id)
);

CREATE INDEX IF NOT EXISTS idx_user_profiles_user_id
    ON user_profiles (user_id);

CREATE INDEX IF NOT EXISTS idx_user_badges_user_id
    ON user_badges (user_id);

CREATE INDEX IF NOT EXISTS idx_user_badges_badge_id
    ON user_badges (badge_id);

CREATE INDEX IF NOT EXISTS idx_hunt_participants_user_id
    ON hunt_participants(user_id);

CREATE INDEX IF NOT EXISTS idx_hunt_participants_hunt_id
    ON hunt_participants(hunt_id);
