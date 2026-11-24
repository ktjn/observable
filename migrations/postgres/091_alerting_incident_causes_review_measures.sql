-- ============================================================================
-- Migration 091: confirmed incident causes and approved review measures
-- (observable-alerting, W5a, docs/assurance-relocations.md, OBS-09-07, 11-05, 17-06)
-- ============================================================================

ALTER TABLE alerting.incidents ADD COLUMN IF NOT EXISTS confirmed_cause    TEXT;
ALTER TABLE alerting.incidents ADD COLUMN IF NOT EXISTS cause_category     TEXT;
ALTER TABLE alerting.incidents ADD COLUMN IF NOT EXISTS cause_confirmed_by UUID;
ALTER TABLE alerting.incidents ADD COLUMN IF NOT EXISTS cause_confirmed_at TIMESTAMPTZ;

-- Which measures a tenant admin approved for each review; nothing else is reported.
CREATE TABLE IF NOT EXISTS alerting.review_measures (
    tenant_id   UUID        NOT NULL,
    review      TEXT        NOT NULL CHECK (review IN ('delivery', 'benefits')),
    measures    TEXT[]      NOT NULL,
    approved_by UUID        NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, review)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON alerting.review_measures TO observable_alerting_role;
