-- ============================================================================
-- Migration 052: decide-access default grants for catalog journeys
--
-- Policy data in the auth schema (owned by observable-auth) for the
-- `catalog.journey` resource type (observable-catalog phase 1 S4). Same role
-- ceilings as `catalog.service` (migration 047).
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'catalog.journey', 'read',  'internal'),
    (NULL, 'member',       'catalog.journey', 'read',  'confidential'),
    (NULL, 'tenant_admin', 'catalog.journey', 'read',  'restricted'),
    (NULL, 'member',       'catalog.journey', 'write', 'internal'),
    (NULL, 'tenant_admin', 'catalog.journey', 'write', 'restricted')
ON CONFLICT DO NOTHING;
