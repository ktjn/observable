CREATE TABLE IF NOT EXISTS observable.logs
(
    tenant_id                    UUID,
    log_id                       UUID,
    timestamp_unix_nano          UInt64,
    observed_timestamp_unix_nano UInt64,
    severity_number              Int32,
    severity_text                LowCardinality(String) DEFAULT '',
    body                         String,
    trace_id                     Nullable(String),
    span_id                      Nullable(String),
    attributes                   String DEFAULT '{}',
    resource_attributes          String DEFAULT '{}',
    service_name                 LowCardinality(String),
    environment                  LowCardinality(String) DEFAULT '',
    host_id                      String DEFAULT '',
    fingerprint                  Nullable(UInt64),
    INDEX idx_trace_id trace_id TYPE bloom_filter(0.01) GRANULARITY 1,
    INDEX idx_severity severity_number TYPE minmax GRANULARITY 1
)
ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(timestamp_unix_nano)))
ORDER BY (tenant_id, service_name, timestamp_unix_nano, log_id)
TTL fromUnixTimestamp64Nano(timestamp_unix_nano) + INTERVAL 60 DAY
SETTINGS index_granularity = 8192;
