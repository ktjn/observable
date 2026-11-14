-- ============================================================================
-- Migration 075: Allow 'escalated' event_type in incident_events
-- (observable-alerting, phase 5 S1, OBS-07-04, OBS-07-08)
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
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
            'CHECK (event_type IN (''triggered'', ''acknowledged'', ''escalated'', ''comment'', ''status_change'', ''deployment_linked'', ''alert_fired'', ''alert_resolved''))',
            target.table_schema, target.table_name
        );
    END LOOP;
END $$;
