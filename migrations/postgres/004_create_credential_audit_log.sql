-- Append-only audit log for API key validation events.
-- Application code only INSERTs; no UPDATE or DELETE is issued by the service.
CREATE TABLE IF NOT EXISTS credential_audit_log (
    id               UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    occurred_at      TIMESTAMPTZ NOT NULL    DEFAULT now(),
    action           TEXT        NOT NULL,           -- always 'credential_validate'
    outcome          TEXT        NOT NULL,           -- 'allow' | 'deny'
    credential_hash  TEXT        NOT NULL,           -- SHA-256 of the presented API key
    tenant_id        UUID,                           -- NULL when key is not found
    denial_reason    TEXT                            -- NULL when outcome = 'allow'
);

CREATE INDEX IF NOT EXISTS cred_audit_occurred_at_idx
    ON credential_audit_log (occurred_at);

CREATE INDEX IF NOT EXISTS cred_audit_tenant_id_idx
    ON credential_audit_log (tenant_id)
    WHERE tenant_id IS NOT NULL;
