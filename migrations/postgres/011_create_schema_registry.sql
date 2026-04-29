-- Schema Registry: structural field catalog (global, platform-level)
-- Semantic annotations are tenant-scoped overlays on top of this catalog.
-- See spec/03-storage.md §5.4 and §5.4.1, ADR-021.

CREATE TABLE IF NOT EXISTS schema_entries (
    id                BIGSERIAL   PRIMARY KEY,
    signal_type       TEXT        NOT NULL CHECK (signal_type IN ('traces', 'logs', 'metrics', 'profiles', 'events', 'deployments')),
    field_name        TEXT        NOT NULL,
    field_type        TEXT        NOT NULL,
    otel_spec_version TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (signal_type, field_name)
);

CREATE INDEX IF NOT EXISTS schema_entries_signal_type_idx ON schema_entries (signal_type);

-- Semantic annotations: optional, operator-authored overlays per tenant.
-- Tenant-scoped so different tenants can describe the same field name differently.
-- metric_type extensions (metric_type, timestamp_column, unit, recommended_downsampling)
-- are required by the MCP server for correct time-series SQL generation (ADR-021).
CREATE TABLE IF NOT EXISTS semantic_annotations (
    id                      BIGSERIAL   PRIMARY KEY,
    tenant_id               UUID        NOT NULL,
    signal_type             TEXT        NOT NULL,
    field_name              TEXT        NOT NULL,
    display_name            TEXT,
    business_description    TEXT,
    owner_team              TEXT,
    interpretation_rule     TEXT        CHECK (interpretation_rule IN ('higher_is_worse', 'higher_is_better', 'directional', 'contextual')),
    effective_sample_rate   FLOAT       CHECK (effective_sample_rate IS NULL OR (effective_sample_rate >= 0 AND effective_sample_rate <= 1)),
    known_derivations       TEXT[]      NOT NULL DEFAULT '{}',
    not_for_billing         BOOLEAN     NOT NULL DEFAULT FALSE,
    -- Metric-type extensions: consumed by MCP server (ADR-021)
    metric_type             TEXT        CHECK (metric_type IN ('counter', 'gauge', 'histogram', 'summary')),
    timestamp_column        TEXT,
    unit                    TEXT,
    recommended_downsampling TEXT,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (tenant_id, signal_type, field_name)
);

CREATE INDEX IF NOT EXISTS semantic_annotations_tenant_signal_idx ON semantic_annotations (tenant_id, signal_type);

-- Dev seed: one metrics schema entry and its annotation for the dev tenant.
-- Provides the metric_type extensions the MCP server reads during NLQ generation.
INSERT INTO schema_entries (signal_type, field_name, field_type, otel_spec_version)
VALUES ('metrics', 'request_duration_ms', 'float64', '1.26.0')
ON CONFLICT DO NOTHING;

INSERT INTO semantic_annotations (
    tenant_id, signal_type, field_name,
    display_name, business_description, owner_team,
    interpretation_rule, effective_sample_rate, not_for_billing,
    metric_type, timestamp_column, unit, recommended_downsampling
) VALUES (
    '00000000-0000-0000-0000-000000000001',
    'metrics',
    'request_duration_ms',
    'Request Duration (ms)',
    'End-to-end HTTP request latency measured at the service boundary, in milliseconds.',
    'platform-team',
    'higher_is_worse',
    1.0,
    TRUE,
    'gauge',
    'timestamp_unix_nano',
    'ms',
    '1m'
) ON CONFLICT DO NOTHING;
