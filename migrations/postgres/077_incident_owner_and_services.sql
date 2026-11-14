-- ============================================================================
-- Migration 077: Incident owner, impact summary, affected services, and
-- timeline milestone event types (observable-alerting, phase 6 S1,
-- OBS-08-01, OBS-08-02, OBS-08-03)
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public')
          AND table_name = 'incidents'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS owner TEXT',
            target.table_schema, target.table_name
        );
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS impact_summary TEXT',
            target.table_schema, target.table_name
        );
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS affected_services TEXT[] NOT NULL DEFAULT ''{}''',
            target.table_schema, target.table_name
        );
    END LOOP;

    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public')
          AND table_name = 'incident_events'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I DROP CONSTRAINT IF EXISTS incident_events_event_type_check',
            target.table_schema, target.table_name
        );
        EXECUTE format(
            'ALTER TABLE %I.%I ADD CONSTRAINT incident_events_event_type_check '
            'CHECK (event_type IN (''triggered'', ''acknowledged'', ''escalated'', ''mitigated'', ''milestone'', ''exception'', ''comment'', ''status_change'', ''deployment_linked'', ''alert_fired'', ''alert_resolved''))',
            target.table_schema, target.table_name
        );
    END LOOP;
END $$;
