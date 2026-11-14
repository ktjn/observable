-- ============================================================================
-- Migration 074: SLO governance, SLI outcome/scope, commitment tier, and approval
-- (observable-alerting, phase 4 S2, OBS-06-01, OBS-06-02, OBS-06-03)
-- ============================================================================

DO $$
DECLARE
    target record;
BEGIN
    FOR target IN
        SELECT table_schema, table_name FROM information_schema.tables
        WHERE table_schema IN ('alerting', 'public')
          AND table_name = 'slo_definitions'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I.%I '
            'ADD COLUMN IF NOT EXISTS sli_outcome TEXT NOT NULL DEFAULT '''', '
            'ADD COLUMN IF NOT EXISTS sli_scope TEXT NOT NULL DEFAULT '''', '
            'ADD COLUMN IF NOT EXISTS commitment_tier TEXT NOT NULL DEFAULT ''internal_slo'', '
            'ADD COLUMN IF NOT EXISTS approval_status TEXT NOT NULL DEFAULT ''draft'', '
            'ADD COLUMN IF NOT EXISTS approved_by UUID, '
            'ADD COLUMN IF NOT EXISTS approved_at TIMESTAMPTZ',
            target.table_schema, target.table_name
        );
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conname = 'slo_definitions_commitment_tier_check'
              AND conrelid = format('%I.%I', target.table_schema, target.table_name)::regclass
        ) THEN
            EXECUTE format(
                'ALTER TABLE %I.%I ADD CONSTRAINT slo_definitions_commitment_tier_check '
                'CHECK (commitment_tier IN (''objective'', ''internal_slo'', ''contractual_sla''))',
                target.table_schema, target.table_name
            );
        END IF;
        IF NOT EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conname = 'slo_definitions_approval_status_check'
              AND conrelid = format('%I.%I', target.table_schema, target.table_name)::regclass
        ) THEN
            EXECUTE format(
                'ALTER TABLE %I.%I ADD CONSTRAINT slo_definitions_approval_status_check '
                'CHECK (approval_status IN (''draft'', ''approved''))',
                target.table_schema, target.table_name
            );
        END IF;
    END LOOP;
END $$;
