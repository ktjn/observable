-- ============================================================================
-- Migration 082: Alert & asset governance P1 (OBS-34-02, OBS-34-04, OBS-34-05)
--
-- Adds:
--   * alert_rule_versions (threshold/condition versioning + restore)
--   * alert_rule_templates (approved cross-environment critical alert sets)
--   * dashboards.last_viewed_at + owner columns if absent (quarterly asset review)
-- ============================================================================

CREATE TABLE IF NOT EXISTS alert_rule_versions (
    version_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id      UUID NOT NULL,
    rule_id        UUID NOT NULL REFERENCES alert_rules(rule_id) ON DELETE CASCADE,
    version_number INT NOT NULL,
    condition      JSONB NOT NULL,
    severity       TEXT NOT NULL,
    changed_by     UUID NOT NULL,
    change_reason  TEXT NOT NULL DEFAULT '',
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (rule_id, version_number)
);

CREATE INDEX IF NOT EXISTS alert_rule_versions_rule_idx
    ON alert_rule_versions (tenant_id, rule_id, version_number DESC);

CREATE TABLE IF NOT EXISTS alert_rule_templates (
    template_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id   UUID NOT NULL,
    name        TEXT NOT NULL CHECK (length(trim(name)) > 0),
    approved    BOOLEAN NOT NULL DEFAULT false,
    approved_by UUID,
    rules       JSONB NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS alert_rule_templates_name_uq
    ON alert_rule_templates (tenant_id, lower(name));

ALTER TABLE dashboards
    ADD COLUMN IF NOT EXISTS last_viewed_at TIMESTAMPTZ,
    ADD COLUMN IF NOT EXISTS owner_kind TEXT,
    ADD COLUMN IF NOT EXISTS owner_id UUID;
