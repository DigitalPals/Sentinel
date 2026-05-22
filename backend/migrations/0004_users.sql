-- Cybex Sentinel — user accounts and login sessions.
--
-- Adds authentication: a single administrator account is created during
-- onboarding, and login sessions are tracked here so the whole dashboard sits
-- behind a login. Session tokens are stored hashed, never in the clear.

CREATE TABLE users (
    id            BIGSERIAL PRIMARY KEY,
    username      TEXT NOT NULL,
    password_hash TEXT NOT NULL,                       -- argon2id PHC string
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Case-insensitive uniqueness, so 'Admin' and 'admin' cannot both exist.
CREATE UNIQUE INDEX users_username_uq ON users (lower(username));

CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY,                       -- SHA-256 of the cookie token
    user_id    BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX sessions_expires_at ON sessions (expires_at);

-- Onboarding visibility is now derived from real state (no user yet / no
-- sources configured), so the persistent first-run flag is obsolete.
DELETE FROM settings WHERE key = 'onboarding_done';
