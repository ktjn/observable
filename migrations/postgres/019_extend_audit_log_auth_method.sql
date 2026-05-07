-- Track whether a credential event came from an API key or an OIDC session.
-- NULL means the row predates this migration (legacy api_key row).
ALTER TABLE credential_audit_log
    ADD COLUMN IF NOT EXISTS auth_method TEXT;   -- 'api_key' | 'oidc_session' | NULL (legacy)

-- The action column previously only held 'credential_validate'.
-- New values: 'login', 'logout', 'tenant_select'.
-- No enum type change needed — action is TEXT.

COMMENT ON COLUMN credential_audit_log.auth_method IS
    'api_key = ingest token; oidc_session = human login; NULL = legacy row';
