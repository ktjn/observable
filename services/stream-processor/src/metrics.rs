use domain::{
    AggregationTemporality, MetricPoint, MetricSeries, MetricType, Span, StatusCode,
    deterministic_metric_series_id,
};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct MetricKey {
    tenant_id: Uuid,
    service_name: String,
    operation_name: String,
    span_kind: String,
    status_code: String,
    environment: String,
}

struct AggregatorState {
    calls: u64,
    errors: u64,
    durations_ns: Vec<u64>,
}

pub struct SpanMetricsAggregator {
    state: Mutex<HashMap<MetricKey, AggregatorState>>,
    window_start_ns: u64,
}

impl SpanMetricsAggregator {
    pub fn new() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self {
            state: Mutex::new(HashMap::new()),
            window_start_ns: now,
        }
    }

    pub fn record_span(&self, span: &Span, tenant_id: Uuid) {
        let key = MetricKey {
            tenant_id,
            service_name: span.service_name.clone(),
            operation_name: span.operation_name.clone(),
            span_kind: format!("{:?}", span.span_kind).to_uppercase(),
            status_code: format!("{:?}", span.status_code).to_uppercase(),
            environment: span.environment.clone(),
        };

        let mut state = self.state.lock().unwrap();
        let entry = state.entry(key).or_insert(AggregatorState {
            calls: 0,
            errors: 0,
            durations_ns: Vec::new(),
        });

        entry.calls += 1;
        if span.status_code == StatusCode::Error {
            entry.errors += 1;
        }
        entry.durations_ns.push(span.duration_ns);
    }

    pub fn flush(&self) -> (Vec<MetricSeries>, Vec<MetricPoint>) {
        let mut state = self.state.lock().unwrap();
        let mut series = Vec::new();
        let mut points = Vec::new();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        for (key, agg) in state.drain() {
            // 1. calls_total
            let mut calls_series = MetricSeries {
                tenant_id: key.tenant_id,
                metric_name: "span.calls_total".into(),
                metric_type: MetricType::Sum,
                is_monotonic: Some(true),
                aggregation_temporality: Some(AggregationTemporality::Delta),
                service_name: key.service_name.clone(),
                environment: key.environment.clone(),
                attributes: self.make_attributes(&key),
                ..Default::default()
            };
            calls_series.metric_series_id = deterministic_metric_series_id(&calls_series);

            let calls_point = MetricPoint {
                tenant_id: key.tenant_id,
                metric_series_id: calls_series.metric_series_id,
                metric_name: calls_series.metric_name.clone(),
                service_name: key.service_name.clone(),
                time_unix_nano: now,
                start_time_unix_nano: Some(self.window_start_ns),
                value_int: Some(agg.calls as i64),
                ..Default::default()
            };

            series.push(calls_series);
            points.push(calls_point);

            // 2. errors_total
            if agg.errors > 0 {
                let mut errors_series = MetricSeries {
                    tenant_id: key.tenant_id,
                    metric_name: "span.errors_total".into(),
                    metric_type: MetricType::Sum,
                    is_monotonic: Some(true),
                    aggregation_temporality: Some(AggregationTemporality::Delta),
                    service_name: key.service_name.clone(),
                    environment: key.environment.clone(),
                    attributes: self.make_attributes(&key),
                    ..Default::default()
                };
                errors_series.metric_series_id = deterministic_metric_series_id(&errors_series);

                let errors_point = MetricPoint {
                    tenant_id: key.tenant_id,
                    metric_series_id: errors_series.metric_series_id,
                    metric_name: errors_series.metric_name.clone(),
                    service_name: key.service_name.clone(),
                    time_unix_nano: now,
                    start_time_unix_nano: Some(self.window_start_ns),
                    value_int: Some(agg.errors as i64),
                    ..Default::default()
                };

                series.push(errors_series);
                points.push(errors_point);
            }

            // 3. duration_ms (Histogram)
            let mut duration_series = MetricSeries {
                tenant_id: key.tenant_id,
                metric_name: "span.duration_ms".into(),
                metric_type: MetricType::Histogram,
                aggregation_temporality: Some(AggregationTemporality::Delta),
                service_name: key.service_name.clone(),
                environment: key.environment.clone(),
                attributes: self.make_attributes(&key),
                unit: "ms".into(),
                ..Default::default()
            };
            duration_series.metric_series_id = deterministic_metric_series_id(&duration_series);

            let mut durations = agg.durations_ns;
            durations.sort_unstable();

            let count = durations.len() as u64;
            let sum = durations.iter().sum::<u64>() as f64 / 1_000_000.0; // ns to ms

            // Simplified histogram buckets (ms)
            let bounds = vec![
                1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0,
            ];
            let mut bucket_counts = vec![0u64; bounds.len() + 1];
            for d_ns in durations {
                let d_ms = d_ns as f64 / 1_000_000.0;
                let mut found = false;
                for (i, bound) in bounds.iter().enumerate() {
                    if d_ms <= *bound {
                        bucket_counts[i] += 1;
                        found = true;
                        break;
                    }
                }
                if !found {
                    *bucket_counts.last_mut().unwrap() += 1;
                }
            }

            let duration_point = MetricPoint {
                tenant_id: key.tenant_id,
                metric_series_id: duration_series.metric_series_id,
                metric_name: duration_series.metric_name.clone(),
                service_name: key.service_name.clone(),
                time_unix_nano: now,
                start_time_unix_nano: Some(self.window_start_ns),
                histogram_count: Some(count),
                histogram_sum: Some(sum),
                histogram_bucket_counts: Some(bucket_counts),
                histogram_explicit_bounds: Some(bounds),
                ..Default::default()
            };

            series.push(duration_series);
            points.push(duration_point);
        }

        (series, points)
    }

    fn make_attributes(&self, key: &MetricKey) -> HashMap<String, String> {
        let mut attrs = HashMap::new();
        attrs.insert("operation".into(), key.operation_name.clone());
        attrs.insert("span.kind".into(), key.span_kind.clone());
        attrs.insert("status.code".into(), key.status_code.clone());
        attrs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{SpanKind, StatusCode};

    #[test]
    fn test_record_and_flush() {
        let agg = SpanMetricsAggregator::new();
        let tenant_id = Uuid::new_v4();
        let span = Span {
            service_name: "test-service".into(),
            operation_name: "test-op".into(),
            span_kind: SpanKind::Server,
            status_code: StatusCode::Ok,
            duration_ns: 100_000_000, // 100ms
            environment: "prod".into(),
            ..Default::default()
        };

        agg.record_span(&span, tenant_id);
        agg.record_span(
            &Span {
                status_code: StatusCode::Error,
                duration_ns: 200_000_000, // 200ms
                ..span.clone()
            },
            tenant_id,
        );

        let (series, points) = agg.flush();

        // 5 series:
        // Key 1 (OK): calls, duration
        // Key 2 (Error): calls, errors, duration
        assert_eq!(series.len(), 5);
        assert_eq!(points.len(), 5);

        let total_calls: i64 = points
            .iter()
            .filter(|p| p.metric_name == "span.calls_total")
            .map(|p| p.value_int.unwrap())
            .sum();
        assert_eq!(total_calls, 2);

        let total_errors: i64 = points
            .iter()
            .filter(|p| p.metric_name == "span.errors_total")
            .map(|p| p.value_int.unwrap())
            .sum();
        assert_eq!(total_errors, 1);

        let total_duration_count: u64 = points
            .iter()
            .filter(|p| p.metric_name == "span.duration_ms")
            .map(|p| p.histogram_count.unwrap())
            .sum();
        assert_eq!(total_duration_count, 2);
    }
}
