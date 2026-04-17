CREATE TABLE IF NOT EXISTS observable.metric_series
(
    tenant_id               UUID,
    metric_series_id        UUID,
    metric_name             LowCardinality(String),
    description             String DEFAULT '',
    unit                    String DEFAULT '',
    metric_type             LowCardinality(String),
    is_monotonic            Nullable(UInt8),
    aggregation_temporality Nullable(LowCardinality(String)),
    attributes              String DEFAULT '{}',
    resource_attributes     String DEFAULT '{}',
    service_name            LowCardinality(String),
    environment             LowCardinality(String) DEFAULT '',
    created_at              DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree(created_at)
ORDER BY (tenant_id, service_name, metric_name, attributes)
SETTINGS index_granularity = 8192;

CREATE TABLE IF NOT EXISTS observable.metric_points
(
    tenant_id                 UUID,
    metric_series_id          UUID,
    metric_name               LowCardinality(String),
    service_name              LowCardinality(String),
    time_unix_nano            UInt64,
    start_time_unix_nano      Nullable(UInt64),
    value_double              Nullable(Float64),
    value_int                 Nullable(Int64),
    histogram_count           Nullable(UInt64),
    histogram_sum             Nullable(Float64),
    histogram_bucket_counts   Array(UInt64) DEFAULT [],
    histogram_explicit_bounds Array(Float64) DEFAULT [],
    INDEX idx_series metric_series_id TYPE set(0) GRANULARITY 1
)
ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(time_unix_nano)))
ORDER BY (tenant_id, metric_series_id, time_unix_nano)
TTL fromUnixTimestamp64Nano(time_unix_nano) + INTERVAL 14 DAY
SETTINGS index_granularity = 8192;
