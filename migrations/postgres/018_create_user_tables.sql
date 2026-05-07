-- Human user principals linked to Zitadel via idp_subject (Zitadel's user ID / sub claim).
CREATE TABLE IF NOT EXISTS users (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    idp_subject TEXT        UNIQUE NOT NULL,
    email       TEXT        NOT NULL,
    name        TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS users_email_idx ON users (email);

-- Maps a user to a tenant with a coarse role.  One row per user+tenant pair.
-- Fine-grained ReBAC (OpenFGA) is P4-S4; this is the P4-S3 coarse model.
CREATE TABLE IF NOT EXISTS user_tenant_roles (
    user_id    UUID NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    tenant_id  UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    role       TEXT NOT NULL CHECK (role IN ('tenant_admin', 'member', 'viewer')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, tenant_id)
);

-- Server-side session records.  Used for revocation; the JWT itself is short-lived (1h).
CREATE TABLE IF NOT EXISTS user_sessions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id)   ON DELETE CASCADE,
    tenant_id   UUID        NOT NULL REFERENCES tenants(id),
    environment TEXT        NOT NULL,
    issued_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS user_sessions_active_idx
    ON user_sessions (user_id, expires_at)
    WHERE revoked_at IS NULL;
