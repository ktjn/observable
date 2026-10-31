-- ============================================================================
-- Migration 043: Extend saved_views signal_kind Check Constraint
--
-- Extends the CHECK constraint on `control.saved_views.signal_kind` to support
-- 'traces' and 'metrics' in addition to 'logs'.
-- ============================================================================

DO $$
BEGIN
    -- Drop old check constraint if present
    ALTER TABLE control.saved_views
        DROP CONSTRAINT IF EXISTS saved_views_signal_kind_check;

    -- Also check public schema for replay compatibility
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'saved_views'
    ) THEN
        ALTER TABLE public.saved_views
            DROP CONSTRAINT IF EXISTS saved_views_signal_kind_check;
        ALTER TABLE public.saved_views
            ADD CONSTRAINT saved_views_signal_kind_check
            CHECK (signal_kind IN ('logs', 'traces', 'metrics'));
    END IF;

    -- Add updated check constraint on control.saved_views
    ALTER TABLE control.saved_views
        ADD CONSTRAINT saved_views_signal_kind_check
        CHECK (signal_kind IN ('logs', 'traces', 'metrics'));
END $$;
