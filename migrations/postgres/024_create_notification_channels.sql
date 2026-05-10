DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'notification_channel_type') THEN
        CREATE TYPE notification_channel_type AS ENUM ('webhook');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'notification_audit_state') THEN
        CREATE TYPE notification_audit_state AS ENUM ('pending', 'sent', 'failed');
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS notification_channels (
    channel_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id  UUID NOT NULL,
    name       TEXT NOT NULL,
    type       notification_channel_type NOT NULL,
    config     JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS notification_audit_log (
    audit_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id       UUID NOT NULL,
    firing_id       UUID NOT NULL REFERENCES alert_firings(firing_id),
    channel_id      UUID NOT NULL REFERENCES notification_channels(channel_id),
    trigger_state   TEXT NOT NULL CHECK (trigger_state IN ('active', 'resolved')),
    state           notification_audit_state NOT NULL DEFAULT 'pending',
    error_message   TEXT,
    retry_count     INT NOT NULL DEFAULT 0,
    last_attempt_at TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (firing_id, channel_id, trigger_state)
);

CREATE INDEX IF NOT EXISTS notification_channels_tenant_idx ON notification_channels (tenant_id);
CREATE INDEX IF NOT EXISTS notification_audit_log_state_idx ON notification_audit_log (state) WHERE state = 'pending';
