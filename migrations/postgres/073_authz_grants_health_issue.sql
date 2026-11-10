-- ============================================================================
-- Migration 073: decide-access default grants for data-quality issues (phase 2 E4)
--
-- Policy data in the auth schema (owned by observable-auth).
--   health.issue  request-level read of a service's data-quality issues, checked before
--                 any lookup. Evidence contributors are still checked through
--                 health.evidence, so restricted evidence withholds its issues.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'health.issue', 'read', 'public'),
    (NULL, 'member',       'health.issue', 'read', 'public'),
    (NULL, 'tenant_admin', 'health.issue', 'read', 'public')
ON CONFLICT DO NOTHING;
