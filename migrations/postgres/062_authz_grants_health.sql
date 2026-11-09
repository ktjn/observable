-- ============================================================================
-- Migration 062: decide-access default grants for health reads (phase 1 C2.3)
--
-- Policy data in the auth schema (owned by observable-auth).
--   health.assessment  request-level read of a service's current health. Every
--                      read checks it first, for missing and existing IDs alike,
--                      so a policy outage cannot reveal whether a service exists.
--   health.evidence    one resource per rule condition, classified at least as
--                      the service. Same role ceilings as catalog.service.
-- ============================================================================

INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES
    (NULL, 'viewer',       'health.assessment', 'read', 'public'),
    (NULL, 'member',       'health.assessment', 'read', 'public'),
    (NULL, 'tenant_admin', 'health.assessment', 'read', 'public'),
    (NULL, 'viewer',       'health.evidence',   'read', 'internal'),
    (NULL, 'member',       'health.evidence',   'read', 'confidential'),
    (NULL, 'tenant_admin', 'health.evidence',   'read', 'restricted')
ON CONFLICT DO NOTHING;
