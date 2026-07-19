-- The crypto live-data demo app has been removed from the repo; clean up its
-- tenant and API key inserted by 033_add_crypto_demo_tenant.sql.

DELETE FROM api_keys WHERE tenant_id = '00000000-0000-0000-0000-000000000003';
DELETE FROM tenants WHERE id = '00000000-0000-0000-0000-000000000003';
