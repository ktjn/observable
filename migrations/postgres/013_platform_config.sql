-- Platform configuration key-value store.
-- Used to persist runtime configuration (e.g. LLM_API_KEY) that operators
-- set via the Setup page, as an alternative to environment variables.
CREATE TABLE IF NOT EXISTS platform_config (
    key         TEXT        PRIMARY KEY,
    value       TEXT        NOT NULL,
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
