CREATE TABLE IF NOT EXISTS dashboard_grants (
    dashboard_id UUID        NOT NULL REFERENCES dashboards(dashboard_id) ON DELETE CASCADE,
    user_id      UUID        NOT NULL REFERENCES users(id)                ON DELETE CASCADE,
    relation     TEXT        NOT NULL CHECK (relation IN ('owner', 'editor', 'viewer')),
    granted_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (dashboard_id, user_id)
);

CREATE INDEX IF NOT EXISTS dashboard_grants_user_idx
    ON dashboard_grants (user_id, dashboard_id);
