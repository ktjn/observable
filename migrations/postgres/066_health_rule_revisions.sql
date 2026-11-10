-- ============================================================================
-- Migration 066: tenant health rule revisions (phase 2 E1)
--
-- Tenant rules are published through PUT /v1/health/rules as new, immutable revisions.
-- published_by records the publishing user; platform defaults have none. The health role
-- keeps DELETE for tenant purge but loses UPDATE, so a published revision cannot change.
-- ============================================================================

ALTER TABLE health.rules ADD COLUMN IF NOT EXISTS published_by UUID;

ALTER TABLE health.rules DROP CONSTRAINT IF EXISTS health_rules_tenant_publisher;
ALTER TABLE health.rules ADD CONSTRAINT health_rules_tenant_publisher
    CHECK (tenant_id IS NULL OR published_by IS NOT NULL);

-- One rule id per tenant, so a tenant never extends another tenant's revisions.
ALTER TABLE health.rules DROP CONSTRAINT IF EXISTS health_rules_tenant_rule_id;
ALTER TABLE health.rules ADD CONSTRAINT health_rules_tenant_rule_id
    CHECK (tenant_id IS NULL OR rule_id = 'tenant-' || tenant_id::text);

REVOKE UPDATE ON health.rules FROM observable_health_role;
