-- ============================================================================
-- Migration 054: decide-access default grants for catalog coverage policies
--
-- Policy data in the auth schema (owned by observable-auth) for the
-- `catalog.coverage_policy` resource type (observable-catalog phase 1 S5).
-- Every tenant role reads coverage policies; only tenant_admin changes them.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'catalog.coverage_policy', 'read',  'internal'),
    (NULL, 'member',       'catalog.coverage_policy', 'read',  'internal'),
    (NULL, 'tenant_admin', 'catalog.coverage_policy', 'read',  'internal'),
    (NULL, 'tenant_admin', 'catalog.coverage_policy', 'write', 'internal')
ON CONFLICT DO NOTHING;
