use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    pub envelope_id: Uuid,
    pub tenant_id: Uuid,
    pub environment: String,
    pub received_at_unix_nano: u64,
    pub payload: EnvelopePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum EnvelopePayload {
    Spans(Vec<crate::span::Span>),
    Logs(Vec<crate::log::LogRecord>),
    Metrics {
        series: Vec<crate::metric::MetricSeries>,
        points: Vec<crate::metric::MetricPoint>,
    },
}

/// The `telemetry.normalized.v1` wire contract: one already-normalized,
/// tenant-stamped batch published by `observable-process` and consumed by
/// `observable-store-clickhouse`. Unlike `TelemetryEnvelope` (one signal type
/// per message), a normalized batch carries all four signal shapes together
/// since a single stream-processor batch interval typically mixes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NormalizedTelemetryBatch {
    pub spans: Vec<crate::span::Span>,
    pub logs: Vec<crate::log::LogRecord>,
    pub series: Vec<crate::metric::MetricSeries>,
    pub points: Vec<crate::metric::MetricPoint>,
}
