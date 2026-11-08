-- ============================================================================
-- Migration 057: decide-access default grants for catalog providers
--
-- Policy data in the auth schema (owned by observable-auth) for the
-- `catalog.provider` resource type (observable-catalog phase 1 S7). Same role
-- ceilings as `catalog.service` (migration 047).
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'catalog.provider', 'read',  'internal'),
    (NULL, 'member',       'catalog.provider', 'read',  'confidential'),
    (NULL, 'tenant_admin', 'catalog.provider', 'read',  'restricted'),
    (NULL, 'member',       'catalog.provider', 'write', 'internal'),
    (NULL, 'tenant_admin', 'catalog.provider', 'write', 'restricted')
ON CONFLICT DO NOTHING;
