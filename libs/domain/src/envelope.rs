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

/// The `deployment.markers.v1` wire contract: published by admin-service on
/// every deployment-marker create/finish, consumed by ingest-gateway to
/// maintain its in-process deployment-correlation cache without a direct
/// Postgres dependency (Phase 5 "clean ingest",
/// docs/component-decomposition.md). One message per lifecycle transition,
/// not a diff -- `status` always carries the marker's current status, so a
/// consumer only ever needs the latest message per
/// (tenant_id, service_name, environment, service_version) key.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentMarkerEvent {
    pub deployment_id: Uuid,
    pub tenant_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    /// `in_progress` | `success` | `failed` | `rolled_back` -- mirrors
    /// `deployment_markers.status` in `migrations/postgres`.
    pub status: String,
    pub started_at_unix_nano: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AggregationTemporality, LogRecord, MetricPoint, MetricSeries, MetricType, Span, SpanKind,
        StatusCode,
    };
    use serde::{Serialize, de::DeserializeOwned};
    use std::collections::HashMap;

    fn roundtrip_json<T: Serialize + DeserializeOwned>(value: &T) -> (serde_json::Value, T) {
        let encoded = serde_json::to_vec(value).unwrap();
        let decoded: T = serde_json::from_slice(&encoded).unwrap();
        (serde_json::to_value(value).unwrap(), decoded)
    }

    fn span(tenant_id: Uuid) -> Span {
        Span {
            tenant_id,
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
            span_id: "00f067aa0ba902b7".into(),
            parent_span_id: Some("00f067aa0ba902b6".into()),
            service_name: "checkout".into(),
            operation_name: "POST /order".into(),
            span_kind: SpanKind::Server,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_005_000_000,
            duration_ns: 5_000_000,
            status_code: StatusCode::Error,
            attributes: HashMap::from([
                ("http.status_code".into(), serde_json::json!(500)),
                ("retry".into(), serde_json::json!(true)),
            ]),
            environment: "prod".into(),
            ..Default::default()
        }
    }

    fn log(tenant_id: Uuid) -> LogRecord {
        LogRecord {
            tenant_id,
            log_id: Uuid::new_v4(),
            timestamp_unix_nano: 1_700_000_000_000_000_000,
            severity_number: 17,
            severity_text: "ERROR".into(),
            body: serde_json::json!({"msg": "payment failed", "amount": 12.5}),
            trace_id: Some("4bf92f3577b34da6a3ce929d0e0e4736".into()),
            fingerprint: Some(u64::MAX),
            service_name: "checkout".into(),
            ..Default::default()
        }
    }

    fn series_and_point(tenant_id: Uuid) -> (MetricSeries, MetricPoint) {
        let series_id = Uuid::new_v4();
        (
            MetricSeries {
                tenant_id,
                metric_series_id: series_id,
                metric_name: "http.server.duration".into(),
                metric_type: MetricType::Histogram,
                is_monotonic: Some(false),
                aggregation_temporality: Some(AggregationTemporality::Delta),
                attributes: HashMap::from([("route".into(), "/order".into())]),
                ..Default::default()
            },
            MetricPoint {
                tenant_id,
                metric_series_id: series_id,
                metric_name: "http.server.duration".into(),
                time_unix_nano: 1_700_000_000_000_000_000,
                start_time_unix_nano: Some(1_699_999_999_000_000_000),
                histogram_count: Some(3),
                histogram_sum: Some(0.75),
                histogram_bucket_counts: Some(vec![1, 2, 0]),
                histogram_explicit_bounds: Some(vec![0.1, 0.5]),
                ..Default::default()
            },
        )
    }

    #[test]
    fn telemetry_envelope_roundtrip_preserves_tenant_id_and_payload() {
        let tenant_id = Uuid::new_v4();
        let (series, point) = series_and_point(tenant_id);
        for payload in [
            EnvelopePayload::Spans(vec![span(tenant_id)]),
            EnvelopePayload::Logs(vec![log(tenant_id)]),
            EnvelopePayload::Metrics {
                series: vec![series.clone()],
                points: vec![point.clone()],
            },
        ] {
            let envelope = TelemetryEnvelope {
                envelope_id: Uuid::new_v4(),
                tenant_id,
                environment: "prod".into(),
                received_at_unix_nano: 1_700_000_000_000_000_001,
                payload,
            };
            let (before, back) = roundtrip_json(&envelope);
            assert_eq!(back.tenant_id, tenant_id);
            assert_eq!(serde_json::to_value(&back).unwrap(), before);
            let inner_tenants: Vec<Uuid> = match &back.payload {
                EnvelopePayload::Spans(s) => s.iter().map(|x| x.tenant_id).collect(),
                EnvelopePayload::Logs(l) => l.iter().map(|x| x.tenant_id).collect(),
                EnvelopePayload::Metrics { series, points } => series
                    .iter()
                    .map(|x| x.tenant_id)
                    .chain(points.iter().map(|x| x.tenant_id))
                    .collect(),
            };
            assert!(!inner_tenants.is_empty());
            assert!(inner_tenants.iter().all(|t| *t == tenant_id));
        }
    }

    #[test]
    fn normalized_batch_roundtrip_preserves_tenant_ids_and_field_types() {
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();
        let (series, point) = series_and_point(tenant_b);
        let batch = NormalizedTelemetryBatch {
            spans: vec![span(tenant_a)],
            logs: vec![log(tenant_b)],
            series: vec![series],
            points: vec![point],
        };
        let (before, back) = roundtrip_json(&batch);
        assert_eq!(serde_json::to_value(&back).unwrap(), before);
        assert_eq!(back.spans[0].tenant_id, tenant_a);
        assert_eq!(back.spans[0].span_kind, SpanKind::Server);
        assert_eq!(back.spans[0].status_code, StatusCode::Error);
        assert_eq!(back.logs[0].tenant_id, tenant_b);
        assert_eq!(back.logs[0].fingerprint, Some(u64::MAX));
        assert_eq!(back.series[0].tenant_id, tenant_b);
        assert_eq!(back.series[0].metric_type, MetricType::Histogram);
        assert_eq!(back.points[0].tenant_id, tenant_b);
        assert_eq!(back.points[0].histogram_bucket_counts, Some(vec![1, 2, 0]));
        assert_eq!(back.points[0].value_int, None);
    }
}
