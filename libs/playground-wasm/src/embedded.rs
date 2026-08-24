use domain::{
    AggregationTemporality, EnvelopePayload, LogRecord, MetricPoint, MetricSeries, MetricType,
    Span, StatusCode, TelemetryEnvelope, processing::MergedTelemetry,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::transport::InMemoryTransport;

/// Portable stream-processor core used by the browser composition layer.
/// Native stream-processor hosts use the same domain processing functions;
/// this wrapper keeps the WASM binding focused on composition and transport.
#[derive(Debug, Default)]
pub struct EmbeddedStreamProcessor;

#[derive(Debug, Deserialize)]
struct GeneratedSpan {
    trace_id: String,
    span_id: String,
    parent_span_id: String,
    service_name: String,
    operation_name: String,
    duration_ns: String,
    status_code: String,
    environment: String,
    start_time_unix_nano: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedLog {
    timestamp_unix_nano: String,
    observed_timestamp_unix_nano: String,
    severity_number: i32,
    severity_text: String,
    body: String,
    trace_id: String,
    span_id: String,
    service_name: String,
    environment: String,
    host_id: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedMetricSeries {
    metric_series_id: String,
    metric_name: String,
    description: String,
    unit: String,
    metric_type: String,
    is_monotonic: bool,
    aggregation_temporality: String,
    service_name: String,
    environment: String,
}

#[derive(Debug, Deserialize)]
struct GeneratedMetricPoint {
    metric_series_id: String,
    time_unix_nano: String,
    start_time_unix_nano: String,
    value_double: f64,
}

#[derive(Debug, Serialize)]
pub struct ProcessedMetricSeries {
    pub tenant_id: Uuid,
    pub metric_series_id: String,
    pub metric_name: String,
    pub description: String,
    pub unit: String,
    pub metric_type: String,
    pub is_monotonic: bool,
    pub aggregation_temporality: String,
    pub service_name: String,
    pub environment: String,
}

#[derive(Debug, Serialize)]
pub struct ProcessedMetricPoint {
    pub tenant_id: Uuid,
    pub metric_series_id: String,
    pub time_unix_nano: u64,
    pub start_time_unix_nano: u64,
    pub value_double: f64,
}

#[derive(Debug, Serialize)]
pub struct ProcessedLog {
    pub tenant_id: Uuid,
    pub log_id: Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub trace_id: String,
    pub span_id: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ProcessedSpan {
    pub tenant_id: Uuid,
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service_name: String,
    pub operation_name: String,
    pub duration_ns: u64,
    pub status_code: String,
    pub environment: String,
    pub start_time_unix_nano: u64,
}

impl EmbeddedStreamProcessor {
    pub fn process_batch(
        &self,
        batch: impl IntoIterator<Item = TelemetryEnvelope>,
    ) -> MergedTelemetry {
        domain::processing::merge_telemetry(batch)
    }

    /// Converts the playground producer's compact span payload into the
    /// canonical domain contract before it reaches the browser-local store.
    pub fn process_generated_spans(
        json: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<ProcessedSpan>, String> {
        let generated: Vec<GeneratedSpan> =
            serde_json::from_str(json).map_err(|e| e.to_string())?;
        let spans = generated
            .into_iter()
            .map(|span| {
                let start_time_unix_nano = span
                    .start_time_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let duration_ns = span.duration_ns.parse::<u64>().map_err(|e| e.to_string())?;
                let status_code = match span.status_code.as_str() {
                    "ERROR" => StatusCode::Error,
                    "OK" => StatusCode::Ok,
                    _ => StatusCode::Unset,
                };
                let span = Span {
                    tenant_id,
                    trace_id: span.trace_id,
                    span_id: span.span_id,
                    parent_span_id: (!span.parent_span_id.is_empty())
                        .then_some(span.parent_span_id),
                    service_name: span.service_name,
                    operation_name: span.operation_name,
                    start_time_unix_nano,
                    end_time_unix_nano: start_time_unix_nano.saturating_add(duration_ns),
                    duration_ns,
                    status_code,
                    environment: span.environment,
                    ..Default::default()
                };
                Ok(span)
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut transport = InMemoryTransport::new();
        let storage = transport.subscribe("telemetry", 1);
        transport
            .publish(
                "telemetry",
                TelemetryEnvelope {
                    envelope_id: Uuid::new_v4(),
                    tenant_id,
                    environment: spans
                        .first()
                        .map(|span| span.environment.clone())
                        .unwrap_or_default(),
                    received_at_unix_nano: spans
                        .first()
                        .map(|span| span.start_time_unix_nano)
                        .unwrap_or_default(),
                    payload: EnvelopePayload::Spans(spans),
                },
            )
            .map_err(|error| format!("embedded telemetry transport failed: {error:?}"))?;
        let envelope = transport
            .receive(storage)
            .ok_or_else(|| "embedded telemetry transport delivered no span batch".to_string())?;
        let processed = Self.process_batch([envelope]);

        Ok(processed
            .spans
            .into_iter()
            .map(|span| ProcessedSpan {
                tenant_id: span.tenant_id,
                trace_id: span.trace_id,
                span_id: span.span_id,
                parent_span_id: span.parent_span_id,
                service_name: span.service_name,
                operation_name: span.operation_name,
                duration_ns: span.duration_ns,
                status_code: match span.status_code {
                    StatusCode::Error => "ERROR",
                    StatusCode::Ok => "OK",
                    StatusCode::Unset => "UNSET",
                }
                .to_string(),
                environment: span.environment,
                start_time_unix_nano: span.start_time_unix_nano,
            })
            .collect())
    }

    /// Converts generated logs into canonical records before local storage.
    pub fn process_generated_logs(
        json: &str,
        tenant_id: Uuid,
    ) -> Result<Vec<ProcessedLog>, String> {
        let generated: Vec<GeneratedLog> = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let logs = generated
            .into_iter()
            .map(|log| {
                let timestamp_unix_nano = log
                    .timestamp_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                let observed_timestamp_unix_nano = log
                    .observed_timestamp_unix_nano
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                Ok(LogRecord {
                    tenant_id,
                    log_id: Uuid::nil(),
                    timestamp_unix_nano,
                    observed_timestamp_unix_nano,
                    severity_number: log.severity_number,
                    severity_text: log.severity_text,
                    body: serde_json::Value::String(log.body),
                    trace_id: Some(log.trace_id),
                    span_id: Some(log.span_id),
                    service_name: log.service_name,
                    environment: log.environment,
                    host_id: log.host_id,
                    ..Default::default()
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let mut transport = InMemoryTransport::new();
        let storage = transport.subscribe("telemetry", 1);
        transport
            .publish(
                "telemetry",
                TelemetryEnvelope {
                    envelope_id: Uuid::new_v4(),
                    tenant_id,
                    environment: logs
                        .first()
                        .map(|log| log.environment.clone())
                        .unwrap_or_default(),
                    received_at_unix_nano: logs
                        .first()
                        .map(|log| log.timestamp_unix_nano)
                        .unwrap_or_default(),
                    payload: EnvelopePayload::Logs(logs),
                },
            )
            .map_err(|error| format!("embedded telemetry transport failed: {error:?}"))?;
        let envelope = transport
            .receive(storage)
            .ok_or_else(|| "embedded telemetry transport delivered no log batch".to_string())?;
        let processed = Self.process_batch([envelope]);

        Ok(processed
            .logs
            .into_iter()
            .map(|record| ProcessedLog {
                tenant_id: record.tenant_id,
                log_id: record.log_id,
                timestamp_unix_nano: record.timestamp_unix_nano,
                observed_timestamp_unix_nano: record.observed_timestamp_unix_nano,
                severity_number: record.severity_number,
                severity_text: record.severity_text,
                body: record.body.to_string(),
                trace_id: record.trace_id.unwrap_or_default(),
                span_id: record.span_id.unwrap_or_default(),
                service_name: record.service_name,
                environment: record.environment,
                host_id: record.host_id,
            })
            .collect())
    }

    /// Converts generated metric series and points into canonical domain
    /// records before they reach the browser-local store.
    pub fn process_generated_metrics(
        json: &str,
        tenant_id: Uuid,
    ) -> Result<(Vec<ProcessedMetricSeries>, Vec<ProcessedMetricPoint>), String> {
        #[derive(Deserialize)]
        struct GeneratedMetrics {
            series: Vec<GeneratedMetricSeries>,
            points: Vec<GeneratedMetricPoint>,
        }

        let generated: GeneratedMetrics = serde_json::from_str(json).map_err(|e| e.to_string())?;
        let series = generated
            .series
            .into_iter()
            .map(|item| {
                let metric_type = match item.metric_type.as_str() {
                    "sum" => MetricType::Sum,
                    "histogram" => MetricType::Histogram,
                    "exponential_histogram" => MetricType::ExponentialHistogram,
                    "summary" => MetricType::Summary,
                    _ => MetricType::Gauge,
                };
                let aggregation_temporality = match item.aggregation_temporality.as_str() {
                    "delta" => Some(AggregationTemporality::Delta),
                    "cumulative" => Some(AggregationTemporality::Cumulative),
                    _ => None,
                };
                let metric_series_id = item.metric_series_id.clone();
                let series = domain::processing::normalise_metric_series(
                    MetricSeries {
                        tenant_id,
                        metric_series_id: Uuid::new_v4(),
                        metric_name: item.metric_name.clone(),
                        description: item.description.clone(),
                        unit: item.unit.clone(),
                        metric_type,
                        is_monotonic: Some(item.is_monotonic),
                        aggregation_temporality,
                        service_name: item.service_name.clone(),
                        environment: item.environment.clone(),
                        ..Default::default()
                    },
                    tenant_id,
                );
                ProcessedMetricSeries {
                    tenant_id: series.tenant_id,
                    metric_series_id,
                    metric_name: series.metric_name,
                    description: series.description,
                    unit: series.unit,
                    metric_type: item.metric_type,
                    is_monotonic: series.is_monotonic.unwrap_or(false),
                    aggregation_temporality: item.aggregation_temporality,
                    service_name: series.service_name,
                    environment: series.environment,
                }
            })
            .collect();
        let points = generated
            .points
            .into_iter()
            .map(|item| {
                let point = domain::processing::normalise_metric_point(
                    MetricPoint {
                        tenant_id,
                        metric_series_id: Uuid::new_v4(),
                        time_unix_nano: item
                            .time_unix_nano
                            .parse::<u64>()
                            .map_err(|e| e.to_string())?,
                        start_time_unix_nano: Some(
                            item.start_time_unix_nano
                                .parse::<u64>()
                                .map_err(|e| e.to_string())?,
                        ),
                        value_double: Some(item.value_double),
                        ..Default::default()
                    },
                    tenant_id,
                );
                Ok(ProcessedMetricPoint {
                    tenant_id: point.tenant_id,
                    metric_series_id: item.metric_series_id,
                    time_unix_nano: point.time_unix_nano,
                    start_time_unix_nano: point.start_time_unix_nano.unwrap_or_default(),
                    value_double: point.value_double.unwrap_or_default(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok((series, points))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EnvelopePayload, Span};
    use uuid::Uuid;

    #[test]
    fn browser_processor_uses_shared_domain_normalization() {
        let tenant_id = Uuid::new_v4();
        let result = EmbeddedStreamProcessor.process_batch([TelemetryEnvelope {
            envelope_id: Uuid::new_v4(),
            tenant_id,
            environment: "production".into(),
            received_at_unix_nano: 1,
            payload: EnvelopePayload::Spans(vec![Span {
                start_time_unix_nano: 100,
                end_time_unix_nano: 125,
                ..Default::default()
            }]),
        }]);

        assert_eq!(result.spans[0].tenant_id, tenant_id);
        assert_eq!(result.spans[0].duration_ns, 25);
    }

    #[test]
    fn generated_spans_cross_ingest_boundary_before_storage() {
        let tenant_id = Uuid::new_v4();
        let json = r#"[{"trace_id":"trace-1","span_id":"span-1","parent_span_id":"","service_name":"api","operation_name":"GET /","duration_ns":"10","status_code":"OK","environment":"production","start_time_unix_nano":"100"}]"#;

        let processed = EmbeddedStreamProcessor::process_generated_spans(json, tenant_id).unwrap();

        assert_eq!(processed[0].tenant_id, tenant_id);
        assert_eq!(processed[0].parent_span_id, None);
        assert_eq!(processed[0].duration_ns, 10);
    }

    #[test]
    fn generated_logs_cross_ingest_boundary_with_tenant_stamp() {
        let tenant_id = Uuid::new_v4();
        let json = r#"[{"log_id":"ignored","timestamp_unix_nano":"100","observed_timestamp_unix_nano":"101","severity_number":9,"severity_text":"INFO","body":"ready","trace_id":"trace-1","span_id":"span-1","service_name":"api","environment":"production","host_id":"host"}]"#;

        let processed = EmbeddedStreamProcessor::process_generated_logs(json, tenant_id).unwrap();

        assert_eq!(processed[0].tenant_id, tenant_id);
        assert_eq!(processed[0].timestamp_unix_nano, 100);
        assert_eq!(processed[0].body, "\"ready\"");
    }

    #[test]
    fn generated_metrics_cross_ingest_boundary_with_tenant_stamp() {
        let tenant_id = Uuid::new_v4();
        let json = r#"{"series":[{"metric_series_id":"series-1","metric_name":"requests","description":"requests","unit":"1","metric_type":"sum","is_monotonic":true,"aggregation_temporality":"cumulative","service_name":"api","environment":"production"}],"points":[{"metric_series_id":"series-1","time_unix_nano":"100","start_time_unix_nano":"90","value_double":3.0}]}"#;

        let (series, points) =
            EmbeddedStreamProcessor::process_generated_metrics(json, tenant_id).unwrap();

        assert_eq!(series[0].tenant_id, tenant_id);
        assert_eq!(series[0].metric_series_id, "series-1");
        assert_eq!(points[0].tenant_id, tenant_id);
        assert_eq!(points[0].time_unix_nano, 100);
    }
}
