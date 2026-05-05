-- Establish a dedicated 'observable' tenant for self-ingestion telemetry.
--
-- Before this migration all three seed api_keys shared the single dev-tenant
-- (00000000-0000-0000-0000-000000000001).  We want:
--
--   00000000-0000-0000-0000-000000000001  observable   (self-ingestion, always present)
--   00000000-0000-0000-0000-000000000002  dev-tenant   (local dev / testbench data)
--
-- The observable-internal token (environment='observable') stays at ...001.
-- dev-key and dev-viewer-key move to ...002.

-- Step 1: rename the existing ...001 tenant in-place to 'observable'
UPDATE tenants
SET name = 'observable', updated_at = now()
WHERE id = '00000000-0000-0000-0000-000000000001';

-- Step 2: create dev-tenant at the new UUID
INSERT INTO tenants (id, name)
VALUES ('00000000-0000-0000-0000-000000000002', 'dev-tenant')
ON CONFLICT DO NOTHING;

-- Step 3: move dev-key and dev-viewer-key to dev-tenant; observable-internal stays
UPDATE api_keys
SET tenant_id = '00000000-0000-0000-0000-000000000002'
WHERE tenant_id = '00000000-0000-0000-0000-000000000001'
  AND name != 'observable-internal';
