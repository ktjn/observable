-- ============================================================================
-- Migration 060: decide-access grant for the health worker (phase 1 C2.2)
--
-- Policy data in the auth schema (owned by observable-auth). The health worker
-- reads catalog inputs as the `health_worker` service principal after the
-- catalog has validated its tenant-bound credential (migration 059). The role
-- may read service records of every classification and holds no write grant,
-- so its access stays limited to the inputs needed for evaluation.
-- ============================================================================

ALTER TABLE auth.authz_role_grants DROP CONSTRAINT IF EXISTS authz_role_grants_role_check;
ALTER TABLE auth.authz_role_grants ADD CONSTRAINT authz_role_grants_role_check
    CHECK (role IN ('tenant_admin', 'member', 'viewer', 'health_worker'));

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'health_worker', 'catalog.service', 'read', 'restricted')
ON CONFLICT DO NOTHING;
