use std::collections::{HashMap, HashSet};

use opentelemetry_proto::tonic::common::v1::{AnyValue, KeyValue, any_value};
use opentelemetry_proto::tonic::logs::v1::ResourceLogs;
use opentelemetry_proto::tonic::metrics::v1::{ResourceMetrics, metric};
use opentelemetry_proto::tonic::trace::v1::ResourceSpans;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

pub fn any_value_to_json(v: &AnyValue) -> serde_json::Value {
    match &v.value {
        Some(any_value::Value::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(any_value::Value::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(any_value::Value::IntValue(i)) => serde_json::json!(i),
        Some(any_value::Value::DoubleValue(d)) => serde_json::json!(d),
        Some(any_value::Value::BytesValue(b)) => serde_json::Value::String(hex::encode(b)),
        Some(any_value::Value::ArrayValue(arr)) => {
            serde_json::Value::Array(arr.values.iter().map(any_value_to_json).collect())
        }
        Some(any_value::Value::KvlistValue(kv)) => {
            let map: serde_json::Map<String, serde_json::Value> = kv
                .values
                .iter()
                .map(|kv| {
                    (
                        kv.key.clone(),
                        kv.value.as_ref().map(any_value_to_json).unwrap_or_default(),
                    )
                })
                .collect();
            serde_json::Value::Object(map)
        }
        None => serde_json::Value::Null,
        // String-table dictionary encoding is not supported; we don't have
        // access to the dictionary table at this point.
        _ => serde_json::Value::Null,
    }
}

pub fn kv_list_to_map(kvs: &[KeyValue]) -> HashMap<String, serde_json::Value> {
    kvs.iter()
        .map(|kv| {
            (
                kv.key.clone(),
                kv.value.as_ref().map(any_value_to_json).unwrap_or_default(),
            )
        })
        .collect()
}

fn kv_str_value(kv: &KeyValue) -> String {
    match kv.value.as_ref().and_then(|v| v.value.as_ref()) {
        Some(any_value::Value::StringValue(s)) => s.clone(),
        Some(any_value::Value::IntValue(i)) => i.to_string(),
        Some(any_value::Value::DoubleValue(d)) => d.to_string(),
        Some(any_value::Value::BoolValue(b)) => b.to_string(),
        _ => String::new(),
    }
}

pub fn kv_str_map(kvs: &[KeyValue]) -> HashMap<String, String> {
    kvs.iter()
        .map(|kv| (kv.key.clone(), kv_str_value(kv)))
        .collect()
}

fn service_name_from_kv(attrs: &[KeyValue]) -> String {
    string_attr_from_kv(attrs, "service.name")
}

fn string_attr_from_kv(attrs: &[KeyValue], key: &str) -> String {
    attrs
        .iter()
        .find(|kv| kv.key == key)
        .and_then(|kv| kv.value.as_ref())
        .and_then(|v| match &v.value {
            Some(any_value::Value::StringValue(s)) => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Logs
// ---------------------------------------------------------------------------

pub fn proto_logs_to_domain(
    resource_logs: &[ResourceLogs],
    tenant_id: Uuid,
    environment: &str,
) -> Vec<domain::LogRecord> {
    let mut records = Vec::new();
    for rl in resource_logs {
        let resource_attrs = rl
            .resource
            .as_ref()
            .map(|r| r.attributes.as_slice())
            .unwrap_or_default();
        let service_name = service_name_from_kv(resource_attrs);
        let resource_attributes = kv_list_to_map(resource_attrs);

        for scope_log in &rl.scope_logs {
            for lr in &scope_log.log_records {
                records.push(domain::LogRecord {
                    tenant_id,
                    log_id: Uuid::new_v4(),
                    timestamp_unix_nano: lr.time_unix_nano,
                    observed_timestamp_unix_nano: lr.observed_time_unix_nano,
                    severity_number: lr.severity_number,
                    severity_text: lr.severity_text.clone(),
                    body: lr.body.as_ref().map(any_value_to_json).unwrap_or_default(),
                    trace_id: non_empty_hex(&lr.trace_id),
                    span_id: non_empty_hex(&lr.span_id),
                    attributes: kv_list_to_map(&lr.attributes),
                    resource_attributes: resource_attributes.clone(),
                    service_name: service_name.clone(),
                    environment: environment.to_string(),
                    ..Default::default()
                });
            }
        }
    }
    records
}

// ---------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------

pub fn proto_spans_to_domain(
    resource_spans: &[ResourceSpans],
    tenant_id: Uuid,
    environment: &str,
) -> Vec<domain::Span> {
    let mut spans = Vec::new();
    for rs in resource_spans {
        let resource_attrs = rs
            .resource
            .as_ref()
            .map(|r| r.attributes.as_slice())
            .unwrap_or_default();
        let service_name = service_name_from_kv(resource_attrs);
        let resource_attributes = kv_list_to_map(resource_attrs);

        for scope_span in &rs.scope_spans {
            for s in &scope_span.spans {
                let start = s.start_time_unix_nano;
                let end = s.end_time_unix_nano;
                let span_kind = match s.kind {
                    2 => domain::SpanKind::Server,
                    3 => domain::SpanKind::Client,
                    4 => domain::SpanKind::Producer,
                    5 => domain::SpanKind::Consumer,
                    _ => domain::SpanKind::Internal,
                };
                let (status_code, status_message) = s
                    .status
                    .as_ref()
                    .map(|st| {
                        let code = match st.code {
                            1 => domain::StatusCode::Ok,
                            2 => domain::StatusCode::Error,
                            _ => domain::StatusCode::Unset,
                        };
                        (code, st.message.clone())
                    })
                    .unwrap_or_default();

                spans.push(domain::Span {
                    tenant_id,
                    trace_id: hex::encode(&s.trace_id),
                    span_id: hex::encode(&s.span_id),
                    parent_span_id: non_empty_hex(&s.parent_span_id),
                    service_name: service_name.clone(),
                    operation_name: s.name.clone(),
                    span_kind,
                    start_time_unix_nano: start,
                    end_time_unix_nano: end,
                    duration_ns: end.saturating_sub(start),
                    status_code,
                    status_message,
                    attributes: kv_list_to_map(&s.attributes),
                    resource_attributes: resource_attributes.clone(),
                    environment: environment.to_string(),
                    ..Default::default()
                });
                let span_events: Vec<domain::SpanEvent> = s
                    .events
                    .iter()
                    .enumerate()
                    .map(|(i, e)| domain::SpanEvent {
                        tenant_id,
                        trace_id: hex::encode(&s.trace_id),
                        span_id: hex::encode(&s.span_id),
                        event_index: i as u32,
                        name: e.name.clone(),
                        timestamp_unix_nano: e.time_unix_nano,
                        attributes: kv_list_to_map(&e.attributes),
                    })
                    .collect();
                if let Some(span) = spans.last_mut() {
                    span.events = span_events;
                }
            }
        }
    }
    spans
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

pub fn proto_metrics_to_domain(
    resource_metrics: &[ResourceMetrics],
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<domain::MetricSeries>, Vec<domain::MetricPoint>, u64) {
    let mut series_list = Vec::new();
    let mut points_list = Vec::new();
    let mut seen_series = HashSet::new();
    let mut rejected_data_points: u64 = 0;
    let mut exp_hist_logged = false;
    let mut summary_logged = false;

    for rm in resource_metrics {
        let resource_attrs = rm
            .resource
            .as_ref()
            .map(|r| r.attributes.as_slice())
            .unwrap_or_default();
        let service_name = service_name_from_kv(resource_attrs);
        let resource_attributes = kv_list_to_map(resource_attrs);

        for scope_metric in &rm.scope_metrics {
            for m in &scope_metric.metrics {
                let Some(ref data) = m.data else {
                    continue;
                };

                match data {
                    metric::Data::Gauge(g) => {
                        for dp in &g.data_points {
                            let mut series = domain::MetricSeries {
                                tenant_id,
                                metric_name: m.name.clone(),
                                description: m.description.clone(),
                                unit: m.unit.clone(),
                                metric_type: domain::MetricType::Gauge,
                                is_monotonic: None,
                                aggregation_temporality: None,
                                attributes: kv_str_map(&dp.attributes),
                                resource_attributes: resource_attributes.clone(),
                                service_name: service_name.clone(),
                                environment: environment.to_string(),
                                ..Default::default()
                            };
                            series.metric_series_id =
                                domain::deterministic_metric_series_id(&series);
                            if seen_series.insert(series.metric_series_id) {
                                series_list.push(series.clone());
                            }
                            points_list.push(number_data_point_to_domain(
                                dp,
                                series.metric_series_id,
                                &m.name,
                                &service_name,
                                tenant_id,
                            ));
                        }
                    }
                    metric::Data::Sum(s) => {
                        let agg = proto_temporality_to_domain(s.aggregation_temporality);
                        for dp in &s.data_points {
                            let mut series = domain::MetricSeries {
                                tenant_id,
                                metric_name: m.name.clone(),
                                description: m.description.clone(),
                                unit: m.unit.clone(),
                                metric_type: domain::MetricType::Sum,
                                is_monotonic: Some(s.is_monotonic),
                                aggregation_temporality: agg.clone(),
                                attributes: kv_str_map(&dp.attributes),
                                resource_attributes: resource_attributes.clone(),
                                service_name: service_name.clone(),
                                environment: environment.to_string(),
                                ..Default::default()
                            };
                            series.metric_series_id =
                                domain::deterministic_metric_series_id(&series);
                            if seen_series.insert(series.metric_series_id) {
                                series_list.push(series.clone());
                            }
                            points_list.push(number_data_point_to_domain(
                                dp,
                                series.metric_series_id,
                                &m.name,
                                &service_name,
                                tenant_id,
                            ));
                        }
                    }
                    metric::Data::Histogram(h) => {
                        let agg = proto_temporality_to_domain(h.aggregation_temporality);
                        for dp in &h.data_points {
                            let mut series = domain::MetricSeries {
                                tenant_id,
                                metric_name: m.name.clone(),
                                description: m.description.clone(),
                                unit: m.unit.clone(),
                                metric_type: domain::MetricType::Histogram,
                                is_monotonic: None,
                                aggregation_temporality: agg.clone(),
                                attributes: kv_str_map(&dp.attributes),
                                resource_attributes: resource_attributes.clone(),
                                service_name: service_name.clone(),
                                environment: environment.to_string(),
                                ..Default::default()
                            };
                            series.metric_series_id =
                                domain::deterministic_metric_series_id(&series);
                            if seen_series.insert(series.metric_series_id) {
                                series_list.push(series.clone());
                            }
                            points_list.push(domain::MetricPoint {
                                tenant_id,
                                metric_series_id: series.metric_series_id,
                                metric_name: m.name.clone(),
                                service_name: service_name.clone(),
                                time_unix_nano: dp.time_unix_nano,
                                start_time_unix_nano: Some(dp.start_time_unix_nano),
                                histogram_count: Some(dp.count),
                                histogram_sum: dp.sum,
                                histogram_bucket_counts: if dp.bucket_counts.is_empty() {
                                    None
                                } else {
                                    Some(dp.bucket_counts.clone())
                                },
                                histogram_explicit_bounds: if dp.explicit_bounds.is_empty() {
                                    None
                                } else {
                                    Some(dp.explicit_bounds.clone())
                                },
                                ..Default::default()
                            });
                        }
                    }
                    metric::Data::ExponentialHistogram(eh) => {
                        let agg = proto_temporality_to_domain(eh.aggregation_temporality);
                        if !exp_hist_logged {
                            tracing::debug!("ExponentialHistogram: storing count+sum only, bucket detail dropped");
                            exp_hist_logged = true;
                        }
                        for dp in &eh.data_points {
                            let mut series = domain::MetricSeries {
                                tenant_id,
                                metric_name: m.name.clone(),
                                description: m.description.clone(),
                                unit: m.unit.clone(),
                                metric_type: domain::MetricType::ExponentialHistogram,
                                is_monotonic: None,
                                aggregation_temporality: agg.clone(),
                                attributes: kv_str_map(&dp.attributes),
                                resource_attributes: resource_attributes.clone(),
                                service_name: service_name.clone(),
                                environment: environment.to_string(),
                                ..Default::default()
                            };
                            series.metric_series_id =
                                domain::deterministic_metric_series_id(&series);
                            if seen_series.insert(series.metric_series_id) {
                                series_list.push(series.clone());
                            }
                            let has_buckets = dp
                                .positive
                                .as_ref()
                                .is_some_and(|b| !b.bucket_counts.is_empty())
                                || dp
                                    .negative
                                    .as_ref()
                                    .is_some_and(|b| !b.bucket_counts.is_empty());
                            if has_buckets {
                                rejected_data_points += 1;
                            }
                            points_list.push(domain::MetricPoint {
                                tenant_id,
                                metric_series_id: series.metric_series_id,
                                metric_name: m.name.clone(),
                                service_name: service_name.clone(),
                                time_unix_nano: dp.time_unix_nano,
                                start_time_unix_nano: Some(dp.start_time_unix_nano),
                                histogram_count: Some(dp.count),
                                histogram_sum: dp.sum,
                                ..Default::default()
                            });
                        }
                    }
                    metric::Data::Summary(s) => {
                        if !summary_logged {
                            tracing::debug!("Summary: storing count+sum only, quantile values dropped");
                            summary_logged = true;
                        }
                        for dp in &s.data_points {
                            let mut series = domain::MetricSeries {
                                tenant_id,
                                metric_name: m.name.clone(),
                                description: m.description.clone(),
                                unit: m.unit.clone(),
                                metric_type: domain::MetricType::Summary,
                                is_monotonic: None,
                                aggregation_temporality: None,
                                attributes: kv_str_map(&dp.attributes),
                                resource_attributes: resource_attributes.clone(),
                                service_name: service_name.clone(),
                                environment: environment.to_string(),
                                ..Default::default()
                            };
                            series.metric_series_id =
                                domain::deterministic_metric_series_id(&series);
                            if seen_series.insert(series.metric_series_id) {
                                series_list.push(series.clone());
                            }
                            if !dp.quantile_values.is_empty() {
                                rejected_data_points += 1;
                            }
                            points_list.push(domain::MetricPoint {
                                tenant_id,
                                metric_series_id: series.metric_series_id,
                                metric_name: m.name.clone(),
                                service_name: service_name.clone(),
                                time_unix_nano: dp.time_unix_nano,
                                start_time_unix_nano: Some(dp.start_time_unix_nano),
                                histogram_count: Some(dp.count),
                                histogram_sum: Some(dp.sum),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
    }
    (series_list, points_list, rejected_data_points)
}

fn number_data_point_to_domain(
    dp: &opentelemetry_proto::tonic::metrics::v1::NumberDataPoint,
    sid: Uuid,
    metric_name: &str,
    service_name: &str,
    tenant_id: Uuid,
) -> domain::MetricPoint {
    use opentelemetry_proto::tonic::metrics::v1::number_data_point;
    let (value_double, value_int) = match &dp.value {
        Some(number_data_point::Value::AsDouble(d)) => (Some(*d), None),
        Some(number_data_point::Value::AsInt(i)) => (None, Some(*i)),
        None => (None, None),
    };
    domain::MetricPoint {
        tenant_id,
        metric_series_id: sid,
        metric_name: metric_name.to_string(),
        service_name: service_name.to_string(),
        time_unix_nano: dp.time_unix_nano,
        start_time_unix_nano: Some(dp.start_time_unix_nano),
        value_double,
        value_int,
        ..Default::default()
    }
}

fn proto_temporality_to_domain(v: i32) -> Option<domain::AggregationTemporality> {
    match v {
        1 => Some(domain::AggregationTemporality::Delta),
        2 => Some(domain::AggregationTemporality::Cumulative),
        _ => None,
    }
}

fn non_empty_hex(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() || bytes.iter().all(|&b| b == 0) {
        None
    } else {
        Some(hex::encode(bytes))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::{
        common::v1::{AnyValue, KeyValue, any_value},
        metrics::v1::{
            Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics, metric,
            number_data_point,
        },
        resource::v1::Resource,
    };

    fn string_kv(key: &str, value: &str) -> KeyValue {
        KeyValue {
            key: key.to_string(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(value.to_string())),
            }),
            key_strindex: 0,
        }
    }

    fn gauge_resource_metrics() -> Vec<ResourceMetrics> {
        vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "checkout")],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "cpu.usage".to_string(),
                    description: String::new(),
                    unit: "1".to_string(),
                    data: Some(metric::Data::Gauge(Gauge {
                        data_points: vec![NumberDataPoint {
                            attributes: vec![string_kv("core", "0")],
                            start_time_unix_nano: 100,
                            time_unix_nano: 200,
                            exemplars: Vec::new(),
                            flags: 0,
                            value: Some(number_data_point::Value::AsDouble(0.7)),
                        }],
                    })),
                    metadata: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }]
    }

    #[test]
    fn proto_metrics_to_domain_uses_stable_series_id_for_same_series() {
        let tenant = Uuid::nil();
        let payload = gauge_resource_metrics();

        let (first_series, first_points, _) = proto_metrics_to_domain(&payload, tenant, "testbench");
        let (second_series, second_points, _) = proto_metrics_to_domain(&payload, tenant, "testbench");

        assert_eq!(
            first_series[0].metric_series_id,
            second_series[0].metric_series_id
        );
        assert_eq!(
            first_points[0].metric_series_id,
            second_points[0].metric_series_id
        );
    }

    #[test]
    fn exponential_histogram_data_point_maps_count_and_sum() {
        use opentelemetry_proto::tonic::metrics::v1::{
            ExponentialHistogram, ExponentialHistogramDataPoint, Metric, ResourceMetrics,
            ScopeMetrics, metric,
        };
        use opentelemetry_proto::tonic::resource::v1::Resource;

        let payload = vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "svc-exp")],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "latency".to_string(),
                    description: String::new(),
                    unit: "ms".to_string(),
                    data: Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
                        data_points: vec![ExponentialHistogramDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 100,
                            time_unix_nano: 200,
                            count: 42,
                            sum: Some(1000.0),
                            scale: 3,
                            zero_count: 0,
                            positive: None,
                            negative: None,
                            flags: 0,
                            exemplars: vec![],
                            min: None,
                            max: None,
                            zero_threshold: 0.0,
                        }],
                        aggregation_temporality: 2,
                    })),
                    metadata: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let tenant = Uuid::nil();
        let (series, points, rejected) = proto_metrics_to_domain(&payload, tenant, "testbench");

        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_type, domain::MetricType::ExponentialHistogram);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].histogram_count, Some(42));
        assert_eq!(points[0].histogram_sum, Some(1000.0));
        // No non-empty buckets in this data point → rejected_data_points = 0
        assert_eq!(rejected, 0);
    }

    #[test]
    fn exponential_histogram_with_buckets_increments_rejected() {
        use opentelemetry_proto::tonic::metrics::v1::{
            ExponentialHistogram, ExponentialHistogramDataPoint, Metric, ResourceMetrics,
            ScopeMetrics, exponential_histogram_data_point, metric,
        };
        use opentelemetry_proto::tonic::resource::v1::Resource;

        let payload = vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "svc-exp")],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "latency".to_string(),
                    description: String::new(),
                    unit: "ms".to_string(),
                    data: Some(metric::Data::ExponentialHistogram(ExponentialHistogram {
                        data_points: vec![ExponentialHistogramDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 100,
                            time_unix_nano: 200,
                            count: 10,
                            sum: Some(500.0),
                            scale: 3,
                            zero_count: 1,
                            positive: Some(exponential_histogram_data_point::Buckets {
                                offset: 0,
                                bucket_counts: vec![1, 2, 3],
                            }),
                            negative: None,
                            flags: 0,
                            exemplars: vec![],
                            min: None,
                            max: None,
                            zero_threshold: 0.0,
                        }],
                        aggregation_temporality: 2,
                    })),
                    metadata: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let tenant = Uuid::nil();
        let (_series, _points, rejected) = proto_metrics_to_domain(&payload, tenant, "testbench");
        assert_eq!(rejected, 1);
    }

    #[test]
    fn summary_data_point_maps_count_and_sum() {
        use opentelemetry_proto::tonic::metrics::v1::{
            Metric, ResourceMetrics, ScopeMetrics, Summary, SummaryDataPoint, metric,
        };
        use opentelemetry_proto::tonic::resource::v1::Resource;

        let payload = vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "svc-sum")],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "response_time".to_string(),
                    description: String::new(),
                    unit: "ms".to_string(),
                    data: Some(metric::Data::Summary(Summary {
                        data_points: vec![SummaryDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 100,
                            time_unix_nano: 200,
                            count: 99,
                            sum: 4950.0,
                            quantile_values: vec![],
                            flags: 0,
                        }],
                    })),
                    metadata: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let tenant = Uuid::nil();
        let (series, points, rejected) = proto_metrics_to_domain(&payload, tenant, "testbench");

        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_type, domain::MetricType::Summary);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].histogram_count, Some(99));
        assert_eq!(points[0].histogram_sum, Some(4950.0));
        assert_eq!(rejected, 0);
    }

    #[test]
    fn summary_with_quantiles_increments_rejected() {
        use opentelemetry_proto::tonic::metrics::v1::{
            Metric, ResourceMetrics, ScopeMetrics, Summary, SummaryDataPoint,
            metric, summary_data_point,
        };
        use opentelemetry_proto::tonic::resource::v1::Resource;

        let payload = vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![string_kv("service.name", "svc-sum")],
                dropped_attributes_count: 0,
                entity_refs: Vec::new(),
            }),
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: "response_time".to_string(),
                    description: String::new(),
                    unit: "ms".to_string(),
                    data: Some(metric::Data::Summary(Summary {
                        data_points: vec![SummaryDataPoint {
                            attributes: vec![],
                            start_time_unix_nano: 100,
                            time_unix_nano: 200,
                            count: 99,
                            sum: 4950.0,
                            quantile_values: vec![
                                summary_data_point::ValueAtQuantile { quantile: 0.5, value: 50.0 },
                                summary_data_point::ValueAtQuantile { quantile: 0.99, value: 99.0 },
                            ],
                            flags: 0,
                        }],
                    })),
                    metadata: Vec::new(),
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }];

        let tenant = Uuid::nil();
        let (_series, _points, rejected) = proto_metrics_to_domain(&payload, tenant, "testbench");
        assert_eq!(rejected, 1);
    }
}
