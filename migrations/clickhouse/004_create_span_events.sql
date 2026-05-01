CREATE TABLE IF NOT EXISTS observable.span_events (
    tenant_id    UUID,
    trace_id     String,
    span_id      String,
    event_index  UInt32,
    name         String,
    timestamp_unix_nano UInt64,
    attributes   String DEFAULT '{}'
) ENGINE = MergeTree()
PARTITION BY (tenant_id, toYYYYMM(fromUnixTimestamp64Nano(timestamp_unix_nano)))
ORDER BY (tenant_id, trace_id, span_id, event_index)
TTL toDateTime(fromUnixTimestamp64Nano(timestamp_unix_nano)) + INTERVAL 14 DAY
SETTINGS index_granularity = 8192;
