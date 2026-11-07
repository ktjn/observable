-- ============================================================================
-- Migration 049: decide-access default grants for catalog teams
--
-- Policy data in the auth schema (owned by observable-auth) for the
-- `catalog.team` resource type introduced by observable-catalog phase 1 S3.
-- Teams are internal: every tenant role reads them; member and tenant_admin
-- maintain them.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'catalog.team', 'read',  'internal'),
    (NULL, 'member',       'catalog.team', 'read',  'internal'),
    (NULL, 'tenant_admin', 'catalog.team', 'read',  'internal'),
    (NULL, 'member',       'catalog.team', 'write', 'internal'),
    (NULL, 'tenant_admin', 'catalog.team', 'write', 'internal')
ON CONFLICT DO NOTHING;
