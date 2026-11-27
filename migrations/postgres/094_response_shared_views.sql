-- ============================================================================
-- Migration 094: shared operational views (observable-response, W5b,
-- docs/assurance-relocations.md section 4.7, OBS-16-06)
-- ============================================================================

-- One coordination view per incident and creator; participants are active tenant principals.
CREATE TABLE IF NOT EXISTS response.shared_views (
    view_id     UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID        NOT NULL,
    incident_id UUID        NOT NULL,
    created_by  UUID        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS shared_views_incident_idx
    ON response.shared_views (tenant_id, incident_id);

CREATE TABLE IF NOT EXISTS response.shared_view_participants (
    view_id      UUID        NOT NULL REFERENCES response.shared_views(view_id) ON DELETE CASCADE,
    tenant_id    UUID        NOT NULL,
    principal_id UUID        NOT NULL,
    added_by     UUID        NOT NULL,
    added_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (view_id, principal_id)
);

-- Coordination notes; every participant reads the same timeline. Append-only.
CREATE TABLE IF NOT EXISTS response.shared_view_notes (
    note_id    UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    view_id    UUID        NOT NULL REFERENCES response.shared_views(view_id) ON DELETE CASCADE,
    tenant_id  UUID        NOT NULL,
    author_id  UUID        NOT NULL,
    body       TEXT        NOT NULL CHECK (length(btrim(body)) BETWEEN 1 AND 4000),
    created_at TIMESTAMPTZ NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX IF NOT EXISTS shared_view_notes_view_idx
    ON response.shared_view_notes (view_id, created_at, note_id);

GRANT SELECT, INSERT, DELETE ON response.shared_views, response.shared_view_participants,
    response.shared_view_notes TO observable_response_role;
