-- ============================================================================
-- Migration 055: Accountable owners for alert rules and objectives
-- (observable-alerting, phase 1 S6, OBS-34-01)
--
-- An alert rule or SLO definition records its owner as a user or team
-- principal. Both columns are set or both are null; a null owner is reported
-- as ownerless by the catalog asset registry. Applied to `alerting.*` and to
-- the `public.*` replay placeholders, as in migration 045.
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public')
          AND table_name IN ('alert_rules', 'slo_definitions')
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I ADD COLUMN IF NOT EXISTS owner_kind TEXT, '
            'ADD COLUMN IF NOT EXISTS owner_id UUID',
            target.table_schema, target.table_name
        );
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint c
            JOIN pg_class t ON t.oid = c.conrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            WHERE n.nspname = target.table_schema AND t.relname = target.table_name
              AND c.conname = target.table_name || '_owner_check'
        ) THEN
            EXECUTE format(
                'ALTER TABLE %I.%I ADD CONSTRAINT %I CHECK ('
                '(owner_kind IS NULL AND owner_id IS NULL) OR '
                '(owner_kind IN (''user'', ''team'') AND owner_id IS NOT NULL))',
                target.table_schema, target.table_name, target.table_name || '_owner_check'
            );
        END IF;
    END LOOP;
END $$;
