-- ============================================================================
-- Migration 076: Track runbook review timestamp on alert_rules
-- (observable-alerting, phase 5 S2, OBS-33-01)
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public')
          AND table_name = 'alert_rules'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS runbook_reviewed_at TIMESTAMPTZ',
            target.table_schema, target.table_name
        );
    END LOOP;
END $$;
