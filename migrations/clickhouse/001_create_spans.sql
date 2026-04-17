CREATE TABLE IF NOT EXISTS observable.spans
(
    tenant_id            UUID,
    trace_id             String,
    span_id              String,
    parent_span_id       Nullable(String),
    service_name         LowCardinality(String),
    service_namespace    LowCardinality(String) DEFAULT '',
    service_version      String DEFAULT '',
    operation_name       String,
    span_kind            Enum8('INTERNAL'=0,'SERVER'=1,'CLIENT'=2,'PRODUCER'=3,'CONSUMER'=4) DEFAULT 'INTERNAL',
    start_time_unix_nano UInt64,
    end_time_unix_nano   UInt64,
    duration_ns          UInt64,
    status_code          Enum8('UNSET'=0,'OK'=1,'ERROR'=2) DEFAULT 'UNSET',
    status_message       String DEFAULT '',
    attributes           String DEFAULT '{}',
    resource_attributes  String DEFAULT '{}',
    environment          LowCardinality(String) DEFAULT '',
    host_id              String DEFAULT '',
    workload             String DEFAULT '',
    deployment_id        String DEFAULT '',
    INDEX idx_trace_id trace_id TYPE bloom_filter(0.01) GRANULARITY 1,
    INDEX idx_service  service_name TYPE set(100) GRANULARITY 1
)
ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(start_time_unix_nano)))
ORDER BY (tenant_id, service_name, start_time_unix_nano, trace_id, span_id)
TTL fromUnixTimestamp64Nano(start_time_unix_nano) + INTERVAL 14 DAY
SETTINGS index_granularity = 8192;
