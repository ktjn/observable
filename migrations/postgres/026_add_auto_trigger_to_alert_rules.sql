ALTER TABLE alert_rules
    ADD COLUMN IF NOT EXISTS auto_trigger_incident BOOLEAN NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS auto_trigger_delay_secs BIGINT;
