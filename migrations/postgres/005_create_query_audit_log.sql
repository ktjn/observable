-- Append-only audit log for query read events (traces, logs, metrics).
-- Application code only INSERTs; no UPDATE or DELETE is issued by the service.
CREATE TABLE IF NOT EXISTS query_audit_log (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    occurred_at  TIMESTAMPTZ NOT NULL    DEFAULT now(),
    action       TEXT        NOT NULL,           -- 'trace_get' | 'trace_search' | 'log_search' | 'metric_series_list' | 'metric_points_get'
    tenant_id    UUID        NOT NULL,           -- authenticated tenant making the read
    result_count BIGINT      NOT NULL            -- number of result rows returned; no payload content
);

CREATE INDEX IF NOT EXISTS query_audit_occurred_at_idx
    ON query_audit_log (occurred_at);

CREATE INDEX IF NOT EXISTS query_audit_tenant_id_idx
    ON query_audit_log (tenant_id);
