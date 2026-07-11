# Prometheus Remote Write Receiver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `POST /api/v1/write` to ingest-gateway's platform port (4321) accepting snappy-compressed Prometheus remote_write v1 protobuf, translating it into `domain::MetricSeries` / `MetricPoint` and publishing to the existing Redpanda queue.

**Architecture:** Three new files under `services/ingest-gateway/src/prometheus_rw/` (proto types, translation logic, Axum handler). One route added to `build_platform_router` in `src/http-json/mod.rs`. No new services, no schema changes — the translated metrics reuse the existing OTLP metric pipeline end-to-end.

**Tech Stack:** Rust, Axum, `snap = "1"` (snappy decompression), `prost` (already in workspace), `axum-test` (already in dev-dependencies), `domain::MetricSeries` / `MetricPoint` / `deterministic_metric_series_id`.

**Spec:** `docs/superpowers/specs/2026-06-30-prometheus-remote-write-design.md`

## Global Constraints

- Platform port only (4321) — OTLP port (4318) remains strictly OTLP per ADR-001
- `Content-Type: application/x-protobuf` is the only accepted content-type; anything else → 415
- Success response is `204 No Content` (not 200 — Prometheus agents warn on 200)
- Auth via existing `auth_middleware` + Bearer API key; `X-Tenant-ID` header is ignored
- `snap::raw::Decoder` for decompression (Prometheus sends raw snappy, not framed)
- `observable.ingest_source = "prometheus_remote_write"` on every translated `resource_attributes`
- `_created` suffix series are silently skipped
- Histogram bucket groups missing `+Inf` emit as individual Gauge points rather than being dropped
- No remote_write v2 support in this slice

---

## Files Created / Modified

| File | Action |
|---|---|
| `services/ingest-gateway/Cargo.toml` | Modify — add `snap = "1"` |
| `services/ingest-gateway/src/prometheus_rw/proto.rs` | Create — hand-rolled prost structs |
| `services/ingest-gateway/src/prometheus_rw/convert.rs` | Create — translation logic + unit tests |
| `services/ingest-gateway/src/prometheus_rw/mod.rs` | Create — Axum handler + integration tests |
| `services/ingest-gateway/src/main.rs` | Modify — add `mod prometheus_rw;` |
| `services/ingest-gateway/src/http-json/mod.rs` | Modify — add route to `build_platform_router` |
| `spec/09-api.md` | Modify — document the endpoint |

---

## Task 1: Proto types + snappy dependency

**Files:**
- Modify: `services/ingest-gateway/Cargo.toml`
- Create: `services/ingest-gateway/src/prometheus_rw/proto.rs`

**Interfaces:**
- Produces:
  - `prometheus_rw::proto::WriteRequest` — top-level decoded message
  - `prometheus_rw::proto::TimeSeries` — one metric series with labels + samples
  - `prometheus_rw::proto::Label { name: String, value: String }`
  - `prometheus_rw::proto::Sample { value: f64, timestamp: i64 }` (timestamp in ms)

- [ ] **Step 1: Add `snap` dependency**

In `services/ingest-gateway/Cargo.toml`, add after the `flate2 = "1"` line:

```toml
snap               = "1"
```

- [ ] **Step 2: Create the proto module file**

Create `services/ingest-gateway/src/prometheus_rw/proto.rs`:

```rust
/// Hand-rolled prost Message types for Prometheus remote_write v1.
///
/// Field numbers match the official prometheus/prometheus prompb/types.proto
/// and remote.proto (stable since 2019 — no build.rs needed).

#[derive(prost::Message)]
pub struct WriteRequest {
    #[prost(message, repeated, tag = "1")]
    pub timeseries: Vec<TimeSeries>,
    // tag 3 (metadata) intentionally omitted — not needed for ingestion
}

#[derive(prost::Message)]
pub struct TimeSeries {
    #[prost(message, repeated, tag = "1")]
    pub labels: Vec<Label>,
    #[prost(message, repeated, tag = "2")]
    pub samples: Vec<Sample>,
}

#[derive(prost::Message, Clone)]
pub struct Label {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(prost::Message, Clone)]
pub struct Sample {
    #[prost(double, tag = "1")]
    pub value: f64,
    /// Milliseconds since Unix epoch (not nanoseconds).
    #[prost(int64, tag = "2")]
    pub timestamp: i64,
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;

    #[test]
    fn round_trip_write_request() {
        let original = WriteRequest {
            timeseries: vec![TimeSeries {
                labels: vec![
                    Label { name: "__name__".into(), value: "http_requests_total".into() },
                    Label { name: "job".into(), value: "api-server".into() },
                ],
                samples: vec![Sample { value: 42.0, timestamp: 1_700_000_000_000 }],
            }],
        };

        let mut buf = Vec::new();
        original.encode(&mut buf).unwrap();

        let decoded = WriteRequest::decode(buf.as_slice()).unwrap();
        assert_eq!(decoded.timeseries.len(), 1);
        assert_eq!(decoded.timeseries[0].labels[0].name, "__name__");
        assert_eq!(decoded.timeseries[0].labels[0].value, "http_requests_total");
        assert_eq!(decoded.timeseries[0].samples[0].value, 42.0);
        assert_eq!(decoded.timeseries[0].samples[0].timestamp, 1_700_000_000_000);
    }
}
```

- [ ] **Step 3: Create `src/prometheus_rw/mod.rs` as a stub** (will be filled in Task 3)

Create `services/ingest-gateway/src/prometheus_rw/mod.rs`:

```rust
pub mod convert;
pub mod proto;
```

- [ ] **Step 4: Wire module into `main.rs`**

In `services/ingest-gateway/src/main.rs`, add after the existing `mod` declarations:

```rust
mod prometheus_rw;
```

- [ ] **Step 5: Run the proto round-trip test**

```
cd services/ingest-gateway
cargo test proto::tests::round_trip_write_request
```

Expected: `test proto::tests::round_trip_write_request ... ok`

- [ ] **Step 6: Commit**

```
git add services/ingest-gateway/Cargo.toml services/ingest-gateway/src/prometheus_rw/ services/ingest-gateway/src/main.rs
git commit -m "feat(ingest-gateway): add prometheus remote_write proto types"
```

---

## Task 2: Label-to-attribute translation (`convert.rs`)

**Files:**
- Create: `services/ingest-gateway/src/prometheus_rw/convert.rs`

**Interfaces:**
- Consumes:
  - `proto::WriteRequest`, `proto::TimeSeries`, `proto::Label`, `proto::Sample`
  - `domain::{MetricSeries, MetricPoint, MetricType, AggregationTemporality, deterministic_metric_series_id}`
  - `uuid::Uuid`
- Produces:
  - `pub fn write_request_to_metrics(req: WriteRequest, tenant_id: Uuid, environment: &str) -> (Vec<MetricSeries>, Vec<MetricPoint>)`

**Label mapping rules:**

| Prometheus label | Destination |
|---|---|
| `__name__` | `MetricSeries.metric_name` |
| `job` | `MetricSeries.service_name` (fallback `"unknown"`) |
| `instance` | `resource_attributes["host.name"]` as `serde_json::Value::String` |
| `observable.service_name` | overrides `service_name` if present |
| all others | `MetricSeries.attributes` as `HashMap<String, String>` |

**Type inference from `__name__` suffix:**

| Suffix | `MetricType` | Extra fields |
|---|---|---|
| `_total` | `Sum` | `is_monotonic = Some(true)`, `aggregation_temporality = Some(Cumulative)` |
| `_created` | — | Skip this `TimeSeries` entirely |
| `_bucket` / `_count` / `_sum` | `Histogram` (grouped — see histogram section) | |
| anything else | `Gauge` | |

**Histogram grouping:** Series whose `__name__` ends in `_bucket`, `_count`, or `_sum` are grouped by `(base_name, non_le_non_name_labels)`. For each group, per timestamp:
- `_bucket` samples: sorted by `le` label → `histogram_explicit_bounds` (excluding `+Inf`) + `histogram_bucket_counts`
- `_count` sample: → `histogram_count`
- `_sum` sample: → `histogram_sum`
- Missing `+Inf` bucket → emit each `_bucket` as a Gauge point instead

**Timestamp conversion:** `Sample.timestamp (ms) * 1_000_000 = time_unix_nano`

**Resource attribute on every series:** `resource_attributes["observable.ingest_source"] = serde_json::Value::String("prometheus_remote_write".into())`

**`metric_series_id`:** `domain::deterministic_metric_series_id(&series)` — same as the OTLP path in `http-json/convert.rs`.

- [ ] **Step 1: Write the failing tests first**

Create `services/ingest-gateway/src/prometheus_rw/convert.rs` with tests only (function stub returns empty):

```rust
use std::collections::HashMap;
use uuid::Uuid;

use crate::prometheus_rw::proto::{Label, Sample, TimeSeries, WriteRequest};
use domain::{AggregationTemporality, MetricType, deterministic_metric_series_id};

pub fn write_request_to_metrics(
    req: WriteRequest,
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<domain::MetricSeries>, Vec<domain::MetricPoint>) {
    let _ = (req, tenant_id, environment);
    (vec![], vec![])
}

fn labels_to_map(labels: &[Label]) -> HashMap<String, String> {
    labels.iter().map(|l| (l.name.clone(), l.value.clone())).collect()
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

    fn tenant() -> Uuid { Uuid::parse_str(TENANT).unwrap() }

    fn make_series(labels: Vec<(&str, &str)>, value: f64, ts_ms: i64) -> TimeSeries {
        TimeSeries {
            labels: labels.into_iter().map(|(n, v)| Label { name: n.into(), value: v.into() }).collect(),
            samples: vec![Sample { value, timestamp: ts_ms }],
        }
    }

    #[test]
    fn gauge_series_maps_labels_to_attributes() {
        let req = WriteRequest {
            timeseries: vec![make_series(
                vec![("__name__", "node_cpu_seconds"), ("job", "node"), ("instance", "host1:9100"), ("mode", "idle")],
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
            series[0].resource_attributes.get("observable.ingest_source"),
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
        assert_eq!(series[0].aggregation_temporality, Some(AggregationTemporality::Cumulative));
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
                vec![("__name__", "cpu_usage"), ("job", "node"), ("observable.service_name", "checkout")],
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
        let base_labels = |le: &str| vec![
            ("__name__", "http_request_duration_seconds_bucket"),
            ("job", "api"),
            ("le", le),
        ];
        let req = WriteRequest {
            timeseries: vec![
                make_series(base_labels("0.1"),   5.0,  1_000),
                make_series(base_labels("0.5"),   10.0, 1_000),
                make_series(base_labels("+Inf"),  12.0, 1_000),
                make_series(vec![
                    ("__name__", "http_request_duration_seconds_count"),
                    ("job", "api"),
                ], 12.0, 1_000),
                make_series(vec![
                    ("__name__", "http_request_duration_seconds_sum"),
                    ("job", "api"),
                ], 3.5, 1_000),
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
        assert_eq!(bounds, &[0.1, 0.5]);  // +Inf excluded
        let counts = points[0].histogram_bucket_counts.as_ref().unwrap();
        assert_eq!(counts, &[5, 10, 12]); // all three including +Inf
        assert_eq!(points[0].histogram_count, Some(12));
        assert_eq!(points[0].histogram_sum, Some(3.5));
    }

    #[test]
    fn histogram_without_inf_bucket_emits_as_gauges() {
        // Missing +Inf bucket — cannot reconstruct histogram
        let req = WriteRequest {
            timeseries: vec![
                make_series(vec![
                    ("__name__", "req_duration_seconds_bucket"),
                    ("job", "svc"),
                    ("le", "0.5"),
                ], 3.0, 1_000),
            ],
        };
        let (series, points) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].metric_type, MetricType::Gauge);
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].value_double, Some(3.0));
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
            timeseries: vec![make_series(vec![("__name__", "m"), ("job", "svc")], 1.0, 1_000)],
        };
        let (series, _) = write_request_to_metrics(req, tenant(), ENV);
        assert_eq!(series[0].environment, ENV);
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```
cd services/ingest-gateway
cargo test prometheus_rw::convert::tests
```

Expected: all tests FAIL because `write_request_to_metrics` returns empty vecs.

- [ ] **Step 3: Implement `write_request_to_metrics`**

Replace the stub with the full implementation in `convert.rs`:

```rust
use std::collections::HashMap;
use uuid::Uuid;

use crate::prometheus_rw::proto::{Label, Sample, TimeSeries, WriteRequest};
use domain::{AggregationTemporality, MetricPoint, MetricSeries, MetricType, deterministic_metric_series_id};

pub fn write_request_to_metrics(
    req: WriteRequest,
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<MetricSeries>, Vec<MetricPoint>) {
    // Separate histogram candidates (timeseries whose __name__ ends in _bucket/_count/_sum)
    // from simple series (gauge, sum, skip).
    let mut simple: Vec<TimeSeries> = Vec::new();
    // Key: (base_name, sorted non-le/non-name labels as vec of (name,value))
    let mut histo_groups: HashMap<(String, Vec<(String, String)>), Vec<TimeSeries>> = HashMap::new();

    for ts in req.timeseries {
        let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
        let name = label_map.get("__name__").cloned().unwrap_or_default();

        if name.ends_with("_created") {
            continue;
        }
        if name.ends_with("_bucket") || name.ends_with("_count") || name.ends_with("_sum") {
            let base = base_name(&name).to_string();
            let group_labels: Vec<(String, String)> = ts.labels.iter()
                .filter(|l| l.name != "__name__" && l.name != "le")
                .map(|l| (l.name.clone(), l.value.clone()))
                .collect::<std::collections::BTreeMap<_, _>>()
                .into_iter()
                .collect();
            histo_groups.entry((base, group_labels)).or_default().push(ts);
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
    for ((base_name, _), group_ts) in histo_groups {
        let (series_opt, pts) = convert_histogram_group(&base_name, group_ts, tenant_id, environment);
        if let Some(s) = series_opt {
            all_series.push(s);
            all_points.extend(pts);
        } else {
            // Incomplete histogram — emit as gauges
            all_points.extend(pts);
        }
    }

    all_series
}

fn convert_simple(
    ts: &TimeSeries,
    tenant_id: Uuid,
    environment: &str,
) -> Option<(MetricSeries, Vec<MetricPoint>)> {
    let label_map: HashMap<String, String> = labels_to_map(&ts.labels);
    let metric_name = label_map.get("__name__")?.clone();

    let (metric_type, is_monotonic, aggregation_temporality) = if metric_name.ends_with("_total") {
        (MetricType::Sum, Some(true), Some(AggregationTemporality::Cumulative))
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

    let points: Vec<MetricPoint> = ts.samples.iter().map(|s| MetricPoint {
        tenant_id,
        metric_series_id: series.metric_series_id,
        metric_name: metric_name.clone(),
        service_name: series.service_name.clone(),
        time_unix_nano: (s.timestamp as u64) * 1_000_000,
        value_double: Some(s.value),
        ..Default::default()
    }).collect();

    Some((series, points))
}

fn convert_histogram_group(
    base_name: &str,
    group_ts: Vec<TimeSeries>,
    tenant_id: Uuid,
    environment: &str,
) -> (Option<MetricSeries>, Vec<MetricPoint>) {
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
                let le: f64 = label_map.get("le").and_then(|v| v.parse().ok()).unwrap_or(f64::INFINITY);
                buckets_by_ts.entry(s.timestamp).or_default().push((le, s.value));
            } else if name.ends_with("_count") {
                count_by_ts.insert(s.timestamp, s.value);
            } else if name.ends_with("_sum") {
                sum_by_ts.insert(s.timestamp, s.value);
            }
        }
    }

    // Check if any timestamp has a +Inf bucket; if not, fall back to gauges
    let has_inf = buckets_by_ts.values().any(|bkts| bkts.iter().any(|(le, _)| le.is_infinite()));
    if !has_inf && !buckets_by_ts.is_empty() {
        // Emit as gauges
        let gauge_points: Vec<MetricPoint> = group_ts.iter().flat_map(|ts| {
            let label_map = labels_to_map(&ts.labels);
            let name = label_map.get("__name__").cloned().unwrap_or_default();
            ts.samples.iter().map(move |s| MetricPoint {
                tenant_id,
                metric_name: name.clone(),
                time_unix_nano: (s.timestamp as u64) * 1_000_000,
                value_double: Some(s.value),
                ..Default::default()
            }).collect::<Vec<_>>()
        }).collect();
        return (None, gauge_points);
    }

    let rep_labels = match representative_labels {
        Some(l) => l,
        None => return (None, vec![]),
    };

    let mut series = build_series(
        &rep_labels,
        base_name.to_string(),
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

        let bounds: Vec<f64> = bkts.iter().filter(|(le, _)| !le.is_infinite()).map(|(le, _)| *le).collect();
        let counts: Vec<u64> = bkts.iter().map(|(_, v)| *v as u64).collect();

        let point = MetricPoint {
            tenant_id,
            metric_series_id: series.metric_series_id,
            metric_name: base_name.to_string(),
            service_name: series.service_name.clone(),
            time_unix_nano: (ts_ms as u64) * 1_000_000,
            histogram_explicit_bounds: if bounds.is_empty() { None } else { Some(bounds) },
            histogram_bucket_counts: if counts.is_empty() { None } else { Some(counts) },
            histogram_count: count_by_ts.get(&ts_ms).map(|v| *v as u64),
            histogram_sum: sum_by_ts.get(&ts_ms).copied(),
            ..Default::default()
        };
        points.push(point);
    }

    (Some(series), points)
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

    let attributes: HashMap<String, String> = label_map.iter()
        .filter(|(k, _)| !matches!(k.as_str(), "__name__" | "job" | "instance" | "le" | "observable.service_name"))
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
    labels.iter().map(|l| (l.name.clone(), l.value.clone())).collect()
}

fn base_name(metric_name: &str) -> &str {
    for suffix in &["_bucket", "_count", "_sum", "_created", "_total"] {
        if let Some(base) = metric_name.strip_suffix(suffix) {
            return base;
        }
    }
    metric_name
}
```

- [ ] **Step 4: Run tests to confirm they pass**

```
cd services/ingest-gateway
cargo test prometheus_rw::convert::tests
```

Expected: all 10 tests pass.

- [ ] **Step 5: Commit**

```
git add services/ingest-gateway/src/prometheus_rw/convert.rs
git commit -m "feat(ingest-gateway): add prometheus remote_write translation logic"
```

---

## Task 3: Axum handler + router wiring + spec doc

**Files:**
- Modify: `services/ingest-gateway/src/prometheus_rw/mod.rs` (replace stub)
- Modify: `services/ingest-gateway/src/http-json/mod.rs` (add route)
- Modify: `spec/09-api.md` (document endpoint)

**Interfaces:**
- Consumes:
  - `crate::AppState` (metric_rate_limiter, metric_cardinality, producer)
  - `crate::auth::TenantContext` (Extension from auth_middleware)
  - `prometheus_rw::convert::write_request_to_metrics`
  - `prometheus_rw::proto::WriteRequest`
  - `domain::EnvelopePayload::Metrics`
  - `queue::producer::build_envelope`
  - `snap::raw::Decoder`
  - `prost::Message::decode`
- Produces:
  - `pub async fn write(State, Extension, HeaderMap, Bytes) -> Response` — the route handler

- [ ] **Step 1: Write the failing integration tests**

Replace `services/ingest-gateway/src/prometheus_rw/mod.rs` with:

```rust
pub mod convert;
pub mod proto;

use axum::{
    body::Bytes,
    extract::{Extension, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use prost::Message;

use crate::AppState;
use crate::auth::TenantContext;
use crate::queue::producer::build_envelope;

pub async fn write(
    State(_state): State<AppState>,
    Extension(_ctx): Extension<TenantContext>,
    _headers: HeaderMap,
    _body: Bytes,
) -> Response {
    StatusCode::NOT_IMPLEMENTED.into_response()
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use axum_test::TestServer;
    use prost::Message;

    use crate::AppState;
    use crate::http_json::build_platform_router;
    use crate::readyz::IngestGatewayProbeState;

    const TENANT: &str = "00000000-0000-0000-0000-000000000001";

    fn auth_header() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer dev-api-key-0000"),
        )
    }

    fn prom_content_type() -> (axum::http::HeaderName, axum::http::HeaderValue) {
        (
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/x-protobuf"),
        )
    }

    fn make_snappy_body(timeseries: Vec<super::proto::TimeSeries>) -> Vec<u8> {
        use super::proto::WriteRequest;
        let req = WriteRequest { timeseries };
        let mut proto_bytes = Vec::new();
        req.encode(&mut proto_bytes).unwrap();
        let mut encoder = snap::raw::Encoder::new();
        encoder.compress_vec(&proto_bytes).unwrap()
    }

    fn one_gauge_body() -> Vec<u8> {
        use super::proto::{Label, Sample, TimeSeries};
        make_snappy_body(vec![TimeSeries {
            labels: vec![
                Label { name: "__name__".into(), value: "cpu_usage".into() },
                Label { name: "job".into(), value: "node".into() },
            ],
            samples: vec![Sample { value: 0.5, timestamp: 1_700_000_000_000 }],
        }])
    }

    fn platform_server() -> TestServer {
        let state = AppState::with_stub_auth(TENANT);
        let db = state.db.clone();
        let probe = IngestGatewayProbeState { db };
        TestServer::new(build_platform_router(state, probe))
    }

    #[tokio::test]
    async fn valid_body_returns_204() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn wrong_content_type_returns_415() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )
            .bytes(b"{}".as_ref().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn malformed_snappy_body_returns_400() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_auth_returns_401() {
        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rate_limit_exceeded_returns_429() {
        let state = AppState::with_stub_auth_and_rate_limit(TENANT, 1);
        let db = state.db.clone();
        let probe = IngestGatewayProbeState { db };
        let server = TestServer::new(build_platform_router(state, probe));

        let first = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(first.status_code(), StatusCode::NO_CONTENT);

        let second = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(one_gauge_body().into())
            .await;
        assert_eq!(second.status_code(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(second.headers()["retry-after"], "1");
    }

    #[tokio::test]
    async fn empty_write_request_returns_204() {
        use super::proto::WriteRequest;
        let req = WriteRequest { timeseries: vec![] };
        let mut proto_bytes = Vec::new();
        req.encode(&mut proto_bytes).unwrap();
        let mut encoder = snap::raw::Encoder::new();
        let body = encoder.compress_vec(&proto_bytes).unwrap();

        let server = platform_server();
        let resp = server
            .post("/api/v1/write")
            .add_header(auth_header().0, auth_header().1)
            .add_header(prom_content_type().0, prom_content_type().1)
            .bytes(body.into())
            .await;
        assert_eq!(resp.status_code(), StatusCode::NO_CONTENT);
    }
}
```

- [ ] **Step 2: Run tests to confirm they fail**

```
cd services/ingest-gateway
cargo test prometheus_rw::tests
```

Expected: compilation may fail first (route not wired yet) — that's fine. Add the route first, then re-run.

- [ ] **Step 3: Add route to `build_platform_router`**

In `services/ingest-gateway/src/http-json/mod.rs`, add at the top:

```rust
use crate::prometheus_rw;
```

And inside `build_platform_router`, add to the authenticated sub-router:

```rust
.route("/api/v1/write", post(prometheus_rw::write))
```

The authenticated block should now look like:

```rust
let authenticated = Router::new()
    .route("/v1/deployments", post(deployments::create_deployment))
    .route(
        "/v1/deployments/{deployment_id}",
        axum::routing::patch(deployments::finish_deployment),
    )
    .route(
        "/v1/events/changes",
        post(change_events::create_change_event),
    )
    .route("/api/v1/write", post(prometheus_rw::write))
    .layer(middleware::from_fn_with_state(
        state.clone(),
        auth::auth_middleware,
    ))
    .with_state(state);
```

- [ ] **Step 4: Run tests again — expect `valid_body_returns_204` to fail with 501**

```
cd services/ingest-gateway
cargo test prometheus_rw::tests
```

Expected: `missing_auth_returns_401`, `wrong_content_type_returns_415` pass (auth middleware and type check). `valid_body_returns_204` returns 501. Proceed to implement the handler.

- [ ] **Step 5: Implement the handler**

Replace the stub `write` function in `src/prometheus_rw/mod.rs`:

```rust
pub async fn write(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // Content-type guard
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !content_type.split(';').next().map(str::trim).unwrap_or("").eq_ignore_ascii_case("application/x-protobuf") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    // Snappy decompress
    let mut decoder = snap::raw::Decoder::new();
    let proto_bytes = match decoder.decompress_vec(&body) {
        Ok(b) => b,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Proto decode
    let req = match proto::WriteRequest::decode(proto_bytes.as_slice()) {
        Ok(r) => r,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };

    // Empty request — nothing to do
    if req.timeseries.is_empty() {
        return StatusCode::NO_CONTENT.into_response();
    }

    // Rate limit
    if state.metric_rate_limiter.check_key(&ctx.tenant_id).is_err() {
        tracing::warn!(tenant_id = %ctx.tenant_id, "prometheus remote_write rate limit exceeded");
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [(header::RETRY_AFTER, "1")],
        )
            .into_response();
    }

    // Translate
    let (series, points) = convert::write_request_to_metrics(req, ctx.tenant_id, &ctx.environment);

    state.metric_cardinality.observe(ctx.tenant_id, series.len());

    tracing::info!(
        tenant_id = %ctx.tenant_id,
        series_count = series.len(),
        point_count = points.len(),
        "received prometheus remote_write request"
    );

    if let Some(producer) = &state.producer {
        let envelope = build_envelope(
            ctx.tenant_id,
            &ctx.environment,
            domain::EnvelopePayload::Metrics { series, points },
        );
        if producer.publish(&envelope).await.is_err() {
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }

    StatusCode::NO_CONTENT.into_response()
}
```

- [ ] **Step 6: Run all handler tests**

```
cd services/ingest-gateway
cargo test prometheus_rw::tests
```

Expected: all 6 tests pass.

- [ ] **Step 7: Run the full ingest-gateway test suite**

```
cd services/ingest-gateway
cargo test
```

Expected: all tests pass, no regressions.

- [ ] **Step 8: Update `spec/09-api.md`**

Add the following section after the existing `### Required APIs` list in `spec/09-api.md`:

```markdown
### 14.2 Prometheus Remote Write Ingest

**ADR:** ADR-017-prometheus-remote-write.md

#### Endpoint

`POST /api/v1/write`

Hosted on the ingest-gateway **platform port** (default `4321`). The OTLP port (`4318`) is not affected.

#### Authentication

```
Authorization: Bearer <api-key>
```

Same API key used for OTLP ingest. `X-Tenant-ID` is ignored — tenant is derived from the key.

#### Request

- `Content-Type: application/x-protobuf`
- Body: Prometheus remote_write v1 `WriteRequest` message, snappy-compressed (raw format)

#### Response codes

| Code | Meaning |
|---|---|
| `204 No Content` | Accepted |
| `400 Bad Request` | Snappy or protobuf decode failure |
| `401 Unauthorized` | Missing or invalid API key |
| `403 Forbidden` | API key lacks ingest role |
| `415 Unsupported Media Type` | Wrong Content-Type (including remote_write v2) |
| `429 Too Many Requests` | Rate limit exceeded; retry after `Retry-After` seconds |
| `500 Internal Server Error` | Queue publish failure |

#### Label mapping

| Prometheus label | Observable field |
|---|---|
| `__name__` | metric name |
| `job` | `service_name` |
| `instance` | `resource_attributes["host.name"]` |
| `observable.service_name` | overrides `job` as `service_name` |
| all other labels | `attributes` |

All ingested series carry `resource_attributes["observable.ingest_source"] = "prometheus_remote_write"`.

#### Metric type mapping

| Prometheus pattern | Observable type |
|---|---|
| `_total` suffix | `sum` (monotonic, cumulative) |
| `_bucket` / `_count` / `_sum` group | `histogram` |
| `_created` suffix | dropped |
| everything else | `gauge` |

Remote_write v2 is not supported in this version.
```

- [ ] **Step 9: Commit**

```
git add services/ingest-gateway/src/prometheus_rw/mod.rs services/ingest-gateway/src/http-json/mod.rs spec/09-api.md
git commit -m "feat(ingest-gateway): add POST /api/v1/write prometheus remote_write receiver"
```

---

## Self-Review

**Spec coverage check:**

| Spec section | Covered by |
|---|---|
| Platform port (4321), not OTLP | Task 3 §3: route in `build_platform_router` |
| `Content-Type: application/x-protobuf` guard → 415 | Task 3 handler + test |
| `snap::raw::Decoder` decompression | Task 3 handler |
| prost decode → 400 on failure | Task 3 handler + test |
| API key auth via `auth_middleware` | Task 3: route inside authenticated sub-router + test |
| `X-Tenant-ID` ignored | Not special-cased — auth_middleware ignores it by default ✓ |
| `204 No Content` on success | Task 3 handler + test |
| Empty `WriteRequest` → 204 | Task 3 handler + test |
| `429 + Retry-After: 1` on rate limit | Task 3 handler + test |
| `500` on queue publish failure | Task 3 handler (covered implicitly — no producer in test stub, branch exists in code) |
| Label mapping (job, instance, etc.) | Task 2 `build_series` + tests |
| `observable.service_name` override | Task 2 + test |
| `unknown` fallback for missing job | Task 2 + test |
| `_total` → Sum, monotonic, cumulative | Task 2 + test |
| `_created` → skipped | Task 2 + test |
| Histogram grouping | Task 2 `convert_histogram_group` + test |
| Missing +Inf → Gauge fallback | Task 2 + test |
| Timestamp ms → ns | Task 2 + test |
| `observable.ingest_source` resource attr | Task 2 + test |
| `deterministic_metric_series_id` | Task 2 + test |
| `environment` propagated | Task 2 + test |
| Rate limit reuses `metric_rate_limiter` | Task 3 handler |
| Cardinality budget reuses `metric_cardinality` | Task 3 handler |
| `spec/09-api.md` updated | Task 3 step 8 |
| Remote_write v2 → 415 | Covered by content-type guard (v2 uses different content-type) ✓ |

**Placeholder scan:** No TBDs. All code blocks are complete.

**Type consistency:** `write_request_to_metrics` signature consistent across Task 2 definition and Task 3 call site. `proto::WriteRequest` / `proto::TimeSeries` / `proto::Label` / `proto::Sample` used consistently. `build_platform_router` route matches `pub async fn write` signature.
