-- Tenant purge: auth-service deletes a purged tenant's health worker credentials, so a
-- credential cannot outlive its tenant. Issuing and revoking stay with the migration owner
-- (scripts/health-worker-credential.py); the runtime role still cannot insert or update.
GRANT DELETE ON TABLE auth.health_worker_credentials TO observable_auth_role;
