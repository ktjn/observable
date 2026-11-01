-- ============================================================================
-- Migration 044: Add tenant retention and ingest quota columns
--
-- Adds `retention_days` and `ingest_quota_per_sec` to `control.tenants`
-- (and `public.tenants` placeholder for replay safety), aligning with
-- `spec/system.yaml` tenant-config data model.
-- ============================================================================

DO $$
BEGIN
    -- Check control.tenants
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'control' AND table_name = 'tenants'
    ) THEN
        ALTER TABLE control.tenants
            ADD COLUMN IF NOT EXISTS retention_days INTEGER NOT NULL DEFAULT 30,
            ADD COLUMN IF NOT EXISTS ingest_quota_per_sec INTEGER NOT NULL DEFAULT 100;
    END IF;

    -- Also check public schema for replay compatibility
    IF EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'tenants'
    ) THEN
        ALTER TABLE public.tenants
            ADD COLUMN IF NOT EXISTS retention_days INTEGER NOT NULL DEFAULT 30,
            ADD COLUMN IF NOT EXISTS ingest_quota_per_sec INTEGER NOT NULL DEFAULT 100;
    END IF;
END $$;
