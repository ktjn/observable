-- Add lifecycle timestamps to alert_firings for pending->active promotion and resolution tracking
ALTER TABLE alert_firings ADD COLUMN IF NOT EXISTS firing_start TIMESTAMPTZ NULL;
ALTER TABLE alert_firings ADD COLUMN IF NOT EXISTS resolved_at TIMESTAMPTZ NULL;

CREATE UNIQUE INDEX IF NOT EXISTS alert_firings_one_open_per_rule_idx
    ON alert_firings (rule_id, tenant_id)
    WHERE state IN ('pending', 'active');
