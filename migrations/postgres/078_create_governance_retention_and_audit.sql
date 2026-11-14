-- Governance retention / legal hold policies (OBS-14-03) and unified
-- change audit log for alerts, access policies, and objectives (OBS-14-04).

CREATE TABLE IF NOT EXISTS auth.data_retention_policies (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    data_domain     TEXT NOT NULL,
    classification  TEXT NOT NULL CHECK (classification IN ('public', 'internal', 'confidential', 'restricted')),
    purpose         TEXT NOT NULL,
    retention_days  INTEGER NOT NULL CHECK (retention_days > 0),
    owner_id        TEXT NOT NULL,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (tenant_id, data_domain, classification)
);

CREATE TABLE IF NOT EXISTS auth.data_legal_holds (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    data_domain     TEXT NOT NULL,
    reason          TEXT NOT NULL,
    placed_by       TEXT NOT NULL,
    active          BOOLEAN NOT NULL DEFAULT true,
    placed_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    released_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS data_legal_holds_active_idx
    ON auth.data_legal_holds (tenant_id, data_domain)
    WHERE active = true;

CREATE TABLE IF NOT EXISTS auth.governance_audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    actor_id        TEXT NOT NULL,
    actor_kind      TEXT NOT NULL DEFAULT 'user',
    target_kind     TEXT NOT NULL CHECK (target_kind IN ('alert', 'access_policy', 'objective')),
    target_id       TEXT NOT NULL,
    action          TEXT NOT NULL,
    route           TEXT NOT NULL,
    details         JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS governance_audit_log_lookup_idx
    ON auth.governance_audit_log (tenant_id, target_kind, occurred_at DESC);
