use std::collections::HashMap;
use uuid::Uuid;

use crate::prometheus_rw::proto::{Label, Sample, TimeSeries, WriteRequest};
use domain::{
    AggregationTemporality, MetricPoint, MetricSeries, MetricType, deterministic_metric_series_id,
};

pub fn write_request_to_metrics(
    req: WriteRequest,
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<MetricSeries>, Vec<MetricPoint>) {
    // Separate histogram candidates (timeseries whose __name__ ends in _bucket/_count/_sum)
    // from simple series (gauge, sum, skip).
    let mut simple: Vec<TimeSeries> = Vec::new();
    // Key: (base_name, sorted non-le/non-name labels as vec of (name,value))
    let mut histo_groups: HashMap<(String, Vec<(String, String)>), Vec<TimeSeries>> =
        HashMap::new();

    for ts in req.timeseries {
        let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
        let name = label_map.get("__name__").cloned().unwrap_or_default();

        if name.ends_with("_created") {
            continue;
        }
        if name.ends_with("_bucket") || name.ends_with("_count") || name.ends_with("_sum") {
            let base = base_name(&name).to_string();
            let group_labels: Vec<(String, String)> = ts
                .labels
                .iter()
                .filter(|l| l.name != "__name__" && l.name != "le")
                .map(|l| (l.name.clone(), l.value.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            histo_groups
                .entry((base, group_labels))
                .or_default()
                .push(ts);
        } else {
            simple.push(ts);
        }
    }

    let mut all_series: Vec<MetricSeries> = Vec::new();
    let mut all_points: Vec<MetricPoint> = Vec::new();

    // Process simple series
    for ts in simple {
        if let Some((s, pts)) = convert_simple(&ts, tenant_id, environment) {
            all_series.push(s);
            all_points.extend(pts);
        }
    }

    // Process histogram groups
    for ((base, _), group_ts) in histo_groups {
        let (series_vec, pts) = convert_histogram_group(&base, group_ts, tenant_id, environment);
        all_series.extend(series_vec);
        all_points.extend(pts);
    }

    (all_series, all_points)
}

fn convert_simple(
    ts: &TimeSeries,
    tenant_id: Uuid,
    environment: &str,
) -> Option<(MetricSeries, Vec<MetricPoint>)> {
    let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
    let metric_name = label_map.get("__name__")?.clone();

    let (metric_type, is_monotonic, aggregation_temporality) = if metric_name.ends_with("_total") {
        (
            MetricType::Sum,
            Some(true),
            Some(AggregationTemporality::Cumulative),
        )
    } else {
        (MetricType::Gauge, None, None)
    };

    let mut series = build_series(
        &label_map,
        metric_name.clone(),
        metric_type,
        is_monotonic,
        aggregation_temporality,
        tenant_id,
        environment,
    );
    series.metric_series_id = deterministic_metric_series_id(&series);

    let points: Vec<MetricPoint> = ts
        .samples
        .iter()
        .map(|s| MetricPoint {
            tenant_id,
            metric_series_id: series.metric_series_id,
            metric_name: metric_name.clone(),
            service_name: series.service_name.clone(),
            time_unix_nano: (s.timestamp as u64) * 1_000_000,
            value_double: Some(s.value),
            ..Default::default()
        })
        .collect();

    Some((series, points))
}

fn convert_histogram_group(
    base: &str,
    group_ts: Vec<TimeSeries>,
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<MetricSeries>, Vec<MetricPoint>) {
    // Collect bucket, count, sum samples per timestamp
    // Key: timestamp_ms
    let mut buckets_by_ts: HashMap<i64, Vec<(f64, f64)>> = HashMap::new(); // (le, value)
    let mut count_by_ts: HashMap<i64, f64> = HashMap::new();
    let mut sum_by_ts: HashMap<i64, f64> = HashMap::new();
    let mut representative_labels: Option<HashMap<String, String>> = None;

    for ts in &group_ts {
        let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
        let name = label_map.get("__name__").cloned().unwrap_or_default();

        if representative_labels.is_none() {
            representative_labels = Some(label_map.clone());
        }

        for s in &ts.samples {
            if name.ends_with("_bucket") {
                let le: f64 = label_map
                    .get("le")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(f64::INFINITY);
                buckets_by_ts
                    .entry(s.timestamp)
                    .or_default()
                    .push((le, s.value));
            } else if name.ends_with("_count") {
                count_by_ts.insert(s.timestamp, s.value);
            } else if name.ends_with("_sum") {
                sum_by_ts.insert(s.timestamp, s.value);
            }
        }
    }

    // Check if any timestamp has a +Inf bucket; if not, fall back to gauges
    let has_inf = buckets_by_ts
        .values()
        .any(|bkts| bkts.iter().any(|(le, _)| le.is_infinite()));
    if !has_inf && !buckets_by_ts.is_empty() {
        // Missing +Inf bucket — emit each _bucket TimeSeries as a Gauge.
        // Bug 2 fix: skip _count and _sum; only _bucket series carry useful data here.
        // Bug 1 fix: build the series manually so we can re-insert `le` into attributes
        // before computing the deterministic ID — otherwise two bucket series with different
        // `le` values (e.g. 0.1 and 0.5) would produce identical attributes and collide.
        let mut fallback_series: Vec<MetricSeries> = Vec::new();
        let mut fallback_points: Vec<MetricPoint> = Vec::new();
        for ts in &group_ts {
            let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
            let name = label_map.get("__name__").cloned().unwrap_or_default();
            if !name.ends_with("_bucket") {
                continue;
            }
            let mut series = build_series(
                &label_map,
                name.clone(),
                MetricType::Gauge,
                None,
                None,
                tenant_id,
                environment,
            );
            // Re-insert `le` so each bucket gets a distinct deterministic ID.
            if let Some(le_val) = label_map.get("le") {
                series.attributes.insert("le".to_string(), le_val.clone());
            }
            series.metric_series_id = deterministic_metric_series_id(&series);
            let pts: Vec<MetricPoint> = ts
                .samples
                .iter()
                .map(|s| MetricPoint {
                    tenant_id,
                    metric_series_id: series.metric_series_id,
                    metric_name: name.clone(),
                    service_name: series.service_name.clone(),
                    time_unix_nano: (s.timestamp as u64) * 1_000_000,
                    value_double: Some(s.value),
                    ..Default::default()
                })
                .collect();
            fallback_series.push(series);
            fallback_points.extend(pts);
        }
        return (fallback_series, fallback_points);
    }

    let rep_labels = match representative_labels {
        Some(l) => l,
        None => return (vec![], vec![]),
    };

    let mut series = build_series(
        &rep_labels,
        base.to_string(),
        MetricType::Histogram,
        None,
        None,
        tenant_id,
        environment,
    );
    series.metric_series_id = deterministic_metric_series_id(&series);

    let mut timestamps: Vec<i64> = buckets_by_ts.keys().cloned().collect();
    timestamps.sort();

    let mut points = Vec::new();
    for ts_ms in timestamps {
        let mut bkts = buckets_by_ts.remove(&ts_ms).unwrap_or_default();
        bkts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let bounds: Vec<f64> = bkts
            .iter()
            .filter(|(le, _)| !le.is_infinite())
            .map(|(le, _)| *le)
            .collect();
        let counts: Vec<u64> = bkts.iter().map(|(_, v)| *v as u64).collect();

        let point = MetricPoint {
            tenant_id,
            metric_series_id: series.metric_series_id,
            metric_name: base.to_string(),
            service_name: series.service_name.clone(),
            time_unix_nano: (ts_ms as u64) * 1_000_000,
            histogram_explicit_bounds: if bounds.is_empty() {
                None
            } else {
                Some(bounds)
            },
            histogram_bucket_counts: if counts.is_empty() {
                None
            } else {
                Some(counts)
            },
            histogram_count: count_by_ts.get(&ts_ms).map(|v| *v as u64),
            histogram_sum: sum_by_ts.get(&ts_ms).copied(),
            ..Default::default()
        };
        points.push(point);
    }

    (vec![series], points)
}

fn build_series(
    label_map: &HashMap<String, String>,
    metric_name: String,
    metric_type: MetricType,
    is_monotonic: Option<bool>,
    aggregation_temporality: Option<AggregationTemporality>,
    tenant_id: Uuid,
    environment: &str,
) -> MetricSeries {
    let service_name = label_map
        .get("observable.service_name")
        .or_else(|| label_map.get("job"))
        .cloned()
        .unwrap_or_else(|| "unknown".to_string());

    let mut resource_attributes: HashMap<String, serde_json::Value> = HashMap::new();
    resource_attributes.insert(
        "observable.ingest_source".to_string(),
        serde_json::Value::String("prometheus_remote_write".to_string()),
    );
    if let Some(instance) = label_map.get("instance") {
        resource_attributes.insert(
            "host.name".to_string(),
            serde_json::Value::String(instance.clone()),
        );
    }

    let attributes: HashMap<String, String> = label_map
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k.as_str(),
                "__name__" | "job" | "instance" | "le" | "observable.service_name"
            )
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    MetricSeries {
        tenant_id,
        metric_name,
        metric_type,
        is_monotonic,
        aggregation_temporality,
        attributes,
        resource_attributes,
        service_name,
        environment: environment.to_string(),
        ..Default::default()
    }
}

fn labels_to_map(labels: &[Label]) -> HashMap<String, String> {
    labels
        .iter()
        .map(|l| (l.name.clone(), l.value.clone()))
        .collect()
}

fn base_name(metric_name: &str) -> &str {
    for suffix in &["_bucket", "_count", "_sum", "_created", "_total"] {
        if let Some(base) = metric_name.strip_suffix(suffix) {
            return base;
        }
    }
    metric_name
}

#[cfg(test)]
mod tests {
    use super::*;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";
    const ENV: &str = "production";

    fn tenant() -> Uuid {
        Uuid::parse_str(TENANT).unwrap()
    }

    fn make_series(labels: Vec<(&str, &str)>, value: f64, ts_ms: i64) -> TimeSeries {
        TimeSeries {
            labels: labels
                .into_iter()
                .map(|(n, v)| Label {
                    name: n.into(),
                    value: v.into(),
                })
                .collect(),
            samples: vec![Sample {
                value,
                timestamp: ts_ms,
            }],
        }
    }

    #[test]
    fn gauge_series_maps_labels_to_attributes() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![
                    ("__name__", "node_cpu_seconds"),
                    ("job", "node"),
                    ("instance", "host1:9100"),
                    ("mode", "idle"),
                ],
                1.5,
                1_700_000_000_000,
            )],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_name, "node_cpu_seconds");
        assert_eq!(series[0].metric_type, MetricType::Gauge);
        assert_eq!(series[0].is_monotonic, None);
        assert_eq!(series[0].aggregation_temporality, None);
        assert_eq!(series[0].service_name, "node");
        assert_eq!(series[0].attributes.get("mode"), Some(&"idle".to_string()));
        assert!(!series[0].attributes.contains_key("job"));
        assert!(!series[0].attributes.contains_key("instance"));
        assert!(!series[0].attributes.contains_key("__name__"));
        assert_eq!(
            series[0].resource_attributes.get("host.name"),
            Some(&serde_json::Value::String("host1:9100".into()))
        );
        assert_eq!(
            series[0]
                .resource_attributes
                .get("observable.ingest_source"),
            Some(&serde_json::Value::String("prometheus_remote_write".into()))
        );
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].time_unix_nano, 1_700_000_000_000 * 1_000_000);
        assert_eq!(points[0].value_double, Some(1.5));
    }

    #[test]
    fn total_suffix_maps_to_sum_monotonic_cumulative() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![("__name__", "http_requests_total"), ("job", "api")],
                100.0,
                1_000_000,
            )],
        };
        let (series, _) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series[0].metric_name, "http_requests_total");
        assert_eq!(series[0].metric_type, MetricType::Sum);
        assert_eq!(series[0].is_monotonic, Some(true));
        assert_eq!(
            series[0].aggregation_temporality,
            Some(AggregationTemporality::Cumulative)
        );
    }

    #[test]
    fn created_suffix_is_skipped() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![("__name__", "http_requests_created"), ("job", "api")],
                0.0,
                1_000_000,
            )],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series.len(), 0);
        assert_eq!(points.len(), 0);
    }

    #[test]
    fn observable_service_name_overrides_job() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![
                    ("__name__", "cpu_usage"),
                    ("job", "node"),
                    ("observable.service_name", "checkout"),
                ],
                0.5,
                1_000_000,
            )],
        };
        let (series, _) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series[0].service_name, "checkout");
    }

    #[test]
    fn missing_job_falls_back_to_unknown() {
        let req = WriteRequest {
            timeseries: vec![make_series(vec![("__name__", "cpu_usage")], 0.5, 1_000_000)],
        };
        let (series, _) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series[0].service_name, "unknown");
    }

    #[test]
    fn timestamp_ms_converted_to_ns() {
        let req = WriteRequest {
            timeseries: vec![make_series(vec![("__name__", "m")], 1.0, 1_000)],
        };
        let (_, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(points[0].time_unix_nano, 1_000_000_000);
    }

    #[test]
    fn histogram_buckets_grouped_into_single_series() {
        // Three _bucket timeseries + _count + _sum for the same base name and labels
        let base_labels = |le: &'static str| {
            vec![
                ("__name__", "http_request_duration_seconds_bucket"),
                ("job", "api"),
                ("le", le),
            ]
        };
        let req = WriteRequest {
            timeseries: vec![
                make_series(base_labels("0.1"), 5.0, 1_000),
                make_series(base_labels("0.5"), 10.0, 1_000),
                make_series(base_labels("+Inf"), 12.0, 1_000),
                make_series(
                    vec![
                        ("__name__", "http_request_duration_seconds_count"),
                        ("job", "api"),
                    ],
                    12.0,
                    1_000,
                ),
                make_series(
                    vec![
                        ("__name__", "http_request_duration_seconds_sum"),
                        ("job", "api"),
                    ],
                    3.5,
                    1_000,
                ),
            ],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        // Should collapse into one MetricSeries (the histogram)
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_type, MetricType::Histogram);
        assert_eq!(series[0].metric_name, "http_request_duration_seconds");
        // One point for the single timestamp
        assert_eq!(points.len(), 1);
        let bounds = points[0].histogram_explicit_bounds.as_ref().unwrap();
        assert_eq!(bounds, &[0.1, 0.5]); // +Inf excluded
        let counts = points[0].histogram_bucket_counts.as_ref().unwrap();
        assert_eq!(counts, &[5, 10, 12]); // all three including +Inf
        assert_eq!(points[0].histogram_count, Some(12));
        assert_eq!(points[0].histogram_sum, Some(3.5));
    }

    #[test]
    fn histogram_without_inf_bucket_emits_as_gauges() {
        // Missing +Inf bucket — cannot reconstruct histogram
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![
                    ("__name__", "req_duration_seconds_bucket"),
                    ("job", "svc"),
                    ("le", "0.5"),
                ],
                3.0,
                1_000,
            )],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_type, MetricType::Gauge);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].value_double, Some(3.0));
    }

    #[test]
    fn histogram_fallback_includes_le_in_attributes_for_multi_bucket() {
        // Two _bucket series with different le values, no +Inf → fallback Gauge path
        // Must produce TWO distinct MetricSeries (not one colliding series)
        let req = WriteRequest {
            timeseries: vec![
                make_series(
                    vec![
                        ("__name__", "req_duration_seconds_bucket"),
                        ("job", "svc"),
                        ("le", "0.1"),
                    ],
                    2.0,
                    1_000,
                ),
                make_series(
                    vec![
                        ("__name__", "req_duration_seconds_bucket"),
                        ("job", "svc"),
                        ("le", "0.5"),
                    ],
                    5.0,
                    1_000,
                ),
            ],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(
            series.len(),
            2,
            "each le bucket must produce a distinct series"
        );
        assert_eq!(points.len(), 2);
        // Each series should have le in attributes
        let les: std::collections::HashSet<_> = series
            .iter()
            .filter_map(|s| s.attributes.get("le"))
            .collect();
        assert!(les.contains(&"0.1".to_string()));
        assert!(les.contains(&"0.5".to_string()));
        // All points are Gauge
        for s in &series {
            assert_eq!(s.metric_type, domain::MetricType::Gauge);
        }
    }

    #[test]
    fn deterministic_series_id_is_set() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![("__name__", "cpu"), ("job", "node")],
                1.0,
                1_000,
            )],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_ne!(series[0].metric_series_id, uuid::Uuid::nil());
        assert_eq!(points[0].metric_series_id, series[0].metric_series_id);
    }

    #[test]
    fn environment_is_propagated() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![("__name__", "m"), ("job", "svc")],
                1.0,
                1_000,
            )],
        };
        let (series, _) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series[0].environment, ENV);
    }
}
