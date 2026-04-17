use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    pub envelope_id: Uuid,
    pub tenant_id: Uuid,
    pub received_at_unix_nano: u64,
    pub payload: EnvelopePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EnvelopePayload {
    Spans(Vec<crate::span::Span>),
    Logs(Vec<crate::log::LogRecord>),
    Metrics {
        series: Vec<crate::metric::MetricSeries>,
        points: Vec<crate::metric::MetricPoint>,
    },
}
