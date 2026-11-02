-- ============================================================================
-- Migration 045: Extend incidents lifecycle and responder tracking columns
--
-- Adds `acknowledged_at`, `acknowledged_by`, `resolved_by`, and
-- `related_deployment_id` to `alerting.incidents` (and `public.incidents` for
-- replay safety), aligning with `spec/14-domain-model.md §5` Incident entity.
-- ============================================================================

DO $$
BEGIN
    -- Check alerting.incidents
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'alerting' AND table_name = 'incidents'
    ) THEN
        ALTER TABLE alerting.incidents
            ADD COLUMN IF NOT EXISTS acknowledged_at TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS acknowledged_by TEXT,
            ADD COLUMN IF NOT EXISTS resolved_by TEXT,
            ADD COLUMN IF NOT EXISTS related_deployment_id UUID;
    END IF;

    -- Also check public schema for replay compatibility
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'incidents'
    ) THEN
        ALTER TABLE public.incidents
            ADD COLUMN IF NOT EXISTS acknowledged_at TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS acknowledged_by TEXT,
            ADD COLUMN IF NOT EXISTS resolved_by TEXT,
            ADD COLUMN IF NOT EXISTS related_deployment_id UUID;
    END IF;
END $$;
