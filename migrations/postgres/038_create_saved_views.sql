CREATE TABLE IF NOT EXISTS saved_views (
    saved_view_id  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID        NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    owner_user_id  UUID        REFERENCES users(id) ON DELETE SET NULL,
    name           TEXT        NOT NULL,
    signal_kind    TEXT        NOT NULL CHECK (signal_kind IN ('logs')),
    visibility     TEXT        NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'public')),
    config         JSONB       NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS saved_views_tenant_signal_idx
    ON saved_views (tenant_id, signal_kind, created_at DESC);

CREATE TABLE IF NOT EXISTS saved_view_grants (
    saved_view_id UUID        NOT NULL REFERENCES saved_views(saved_view_id) ON DELETE CASCADE,
    user_id       UUID        NOT NULL REFERENCES users(id)                  ON DELETE CASCADE,
    relation      TEXT        NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (saved_view_id, user_id)
);

CREATE INDEX IF NOT EXISTS saved_view_grants_user_idx
    ON saved_view_grants (user_id, saved_view_id);
