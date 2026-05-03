-- Schema registry entries for shop-worker metrics (added in Phase 2).
-- order_processing_duration_ms: end-to-end time to process a queue message.

INSERT INTO schema_entries (signal_type, field_name, field_type, otel_spec_version)
VALUES ('metrics', 'order_processing_duration_ms', 'float64', '1.26.0')
ON CONFLICT DO NOTHING;

INSERT INTO semantic_annotations (
    tenant_id, signal_type, field_name,
    display_name, business_description, owner_team,
    interpretation_rule, effective_sample_rate, not_for_billing,
    metric_type, timestamp_column, unit, recommended_downsampling
) VALUES (
    '00000000-0000-0000-0000-000000000001',
    'metrics',
    'order_processing_duration_ms',
    'Order Processing Duration (ms)',
    'End-to-end time to process a single order from the queue, in milliseconds.',
    'platform-team',
    'higher_is_worse',
    1.0,
    TRUE,
    'gauge',
    'timestamp_unix_nano',
    'ms',
    '1m'
) ON CONFLICT DO NOTHING;
