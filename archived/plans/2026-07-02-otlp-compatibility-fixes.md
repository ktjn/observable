# OTLP Compatibility Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five wire-protocol gaps in `ingest-gateway` that silently drop data or reject valid OTel SDK connections: HTTP zstd decompression, gRPC gzip compression, ExponentialHistogram/Summary metric type conversion, partial-success response envelope, and configurable gRPC max message size.

**Architecture:** All changes are confined to `services/ingest-gateway` (and the root `Cargo.toml` for a tonic feature flag). The fixes layer cleanly: Tasks 1-2 fix the transport layer (compression), Tasks 3-4 fix the conversion layer (metric types + partial-success threading), Task 5 adds a config knob for message size. No schema changes, no domain struct changes, no migrations.

**Tech Stack:** Rust, tonic 0.14 (`gzip` feature), zstd 0.13, opentelemetry-proto 0.32, axum 0.8

## Global Constraints

- All changes confined to `services/ingest-gateway/` and root `Cargo.toml`
- No new domain types; no ClickHouse schema changes; no DB migrations
- `zstd = "0.13"` — use this exact version string
- tonic gzip feature: add `features = ["gzip"]` to the workspace `tonic = "0.14"` entry in root `Cargo.toml`
- Default `INGEST_GRPC_MAX_MESSAGE_BYTES = 4_194_304` — must not change behavior when env var is unset
- `rejected_data_points` proto field type is `int64` — cast `usize` to `i64` before setting
- Partial-success error message verbatim: `"ExponentialHistogram bucket detail and Summary quantile values not stored"`
- Debug log messages verbatim:
  - `"ExponentialHistogram: storing count+sum only, bucket detail dropped"`
  - `"Summary: storing count+sum only, quantile values dropped"`
- `rejected_data_points` counts 1 per data point that has non-empty buckets/quantile values (not per sub-field)
- Run `cargo test -p ingest-gateway` — all existing tests must pass after every task

---

## File Map

| File | Change |
|------|--------|
| `Cargo.toml` (root) | Add `features = ["gzip"]` to workspace tonic entry |
| `services/ingest-gateway/Cargo.toml` | Add `zstd = "0.13"` dependency |
| `services/ingest-gateway/src/http-json/mod.rs` | Add `"zstd"` arm to `decode_request_body` |
| `services/ingest-gateway/src/grpc/mod.rs` | Add `.accept_compressed(CompressionEncoding::Gzip)` + `.max_decode_message_size(...)` |
| `services/ingest-gateway/src/grpc/convert.rs` | Replace `_ => {}` wildcard with ExponentialHistogram + Summary arms; return `rejected_data_points` from `proto_metrics_to_domain` |
| `services/ingest-gateway/src/grpc/metric.rs` | Thread `rejected_data_points` from convert into `partial_success` response |
| `services/ingest-gateway/src/main.rs` | Read `INGEST_GRPC_MAX_MESSAGE_BYTES` env var; pass to `start_grpc_server` |

---

### Task 1: HTTP zstd decompression

**Files:**
- Modify: `Cargo.toml` (root workspace) — add zstd to workspace? No: zstd is only needed by ingest-gateway, so add it directly to `services/ingest-gateway/Cargo.toml`
- Modify: `services/ingest-gateway/Cargo.toml` — add `zstd = "0.13"`
- Modify: `services/ingest-gateway/src/http-json/mod.rs` — add `"zstd"` arm

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces: `decode_request_body` now handles `"zstd"` encoding (used by Tasks 2-5 indirectly via existing callers)

- [ ] **Step 1: Write failing tests**

Add to `services/ingest-gateway/src/http-json/metrics.rs` inside the existing `#[cfg(test)] mod tests` block, after the last test:

```rust
#[tokio::test]
async fn zstd_compressed_metrics_payload_returns_200() {
    use std::io::Write as _;
    let json = serde_json::to_vec(&two_series_payload()).unwrap();
    let compressed = zstd::encode_all(std::io::Cursor::new(&json), 0).unwrap();

    let app = build_router(AppState::with_stub_auth(TENANT));
    let server = TestServer::new(app);
    let resp = server
        .post("/v1/metrics")
        .add_header(auth_header().0, auth_header().1)
        .add_header(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        )
        .add_header(
            axum::http::header::CONTENT_ENCODING,
            axum::http::HeaderValue::from_static("zstd"),
        )
        .bytes(compressed.into())
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn zstd_corrupt_payload_returns_400() {
    let app = build_router(AppState::with_stub_auth(TENANT));
    let server = TestServer::new(app);
    let resp = server
        .post("/v1/metrics")
        .add_header(auth_header().0, auth_header().1)
        .add_header(
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderValue::from_static("application/json"),
        )
        .add_header(
            axum::http::header::CONTENT_ENCODING,
            axum::http::HeaderValue::from_static("zstd"),
        )
        .bytes(vec![0xDE, 0xAD, 0xBE, 0xEF].into())
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ingest-gateway zstd 2>&1 | tail -20
```

Expected: both tests fail — `zstd` crate is missing, so it won't compile.

- [ ] **Step 3: Add zstd dependency**

In `services/ingest-gateway/Cargo.toml`, add after the `flate2` line:

```toml
zstd               = "0.13"
```

- [ ] **Step 4: Add zstd arm to `decode_request_body`**

In `services/ingest-gateway/src/http-json/mod.rs`, replace:

```rust
fn decode_request_body(headers: &HeaderMap, body: Bytes) -> Result<Vec<u8>, StatusCode> {
    match get_content_encoding(headers) {
        "" | "identity" => Ok(body.to_vec()),
        "gzip" => {
            let mut decoder = GzDecoder::new(body.as_ref());
            let mut decoded = Vec::new();
            decoder
                .read_to_end(&mut decoded)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(decoded)
        }
        _ => Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    }
}
```

with:

```rust
fn decode_request_body(headers: &HeaderMap, body: Bytes) -> Result<Vec<u8>, StatusCode> {
    match get_content_encoding(headers) {
        "" | "identity" => Ok(body.to_vec()),
        "gzip" => {
            let mut decoder = GzDecoder::new(body.as_ref());
            let mut decoded = Vec::new();
            decoder
                .read_to_end(&mut decoded)
                .map_err(|_| StatusCode::BAD_REQUEST)?;
            Ok(decoded)
        }
        "zstd" => zstd::decode_all(std::io::Cursor::new(body.as_ref()))
            .map_err(|_| StatusCode::BAD_REQUEST),
        _ => Err(StatusCode::UNSUPPORTED_MEDIA_TYPE),
    }
}
```

- [ ] **Step 5: Run all ingest-gateway tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -20
```

Expected: all tests pass, including the two new zstd tests.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml services/ingest-gateway/Cargo.toml services/ingest-gateway/src/http-json/mod.rs services/ingest-gateway/src/http-json/metrics.rs
git commit -m "feat(ingest-gateway): add HTTP zstd decompression support"
```

---

### Task 2: gRPC gzip compression

**Files:**
- Modify: `Cargo.toml` (root workspace) — add `features = ["gzip"]` to tonic entry
- Modify: `services/ingest-gateway/src/grpc/mod.rs` — add `.accept_compressed(CompressionEncoding::Gzip)`

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces: tonic server accepts gzip-compressed gRPC request bodies on all services

Note: tonic's `accept_compressed` is server-infrastructure and isn't callable from a test without a live gRPC connection. The test here verifies the server still builds correctly and that existing tests pass, which is the meaningful coverage.

- [ ] **Step 1: Add gzip feature to workspace tonic**

In root `Cargo.toml`, find the line:

```toml
tonic              = "0.14"
```

Replace with:

```toml
tonic              = { version = "0.14", features = ["gzip"] }
```

- [ ] **Step 2: Run tests to verify they still compile and pass**

```bash
cargo test -p ingest-gateway 2>&1 | tail -20
```

Expected: all existing tests pass. The tonic feature flag is additive and should not break anything.

- [ ] **Step 3: Wire compression into gRPC server**

In `services/ingest-gateway/src/grpc/mod.rs`, replace:

```rust
use tonic::transport::Server;
```

with:

```rust
use tonic::codec::CompressionEncoding;
use tonic::transport::Server;
```

Then replace:

```rust
    Server::builder()
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(LogsServiceServer::new(log_service))
        .add_service(MetricsServiceServer::new(metric_service))
        .serve(addr)
        .await?;
```

with:

```rust
    Server::builder()
        .accept_compressed(CompressionEncoding::Gzip)
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(LogsServiceServer::new(log_service))
        .add_service(MetricsServiceServer::new(metric_service))
        .serve(addr)
        .await?;
```

- [ ] **Step 4: Run all ingest-gateway tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml services/ingest-gateway/src/grpc/mod.rs
git commit -m "feat(ingest-gateway): enable gzip compression on gRPC server"
```

---

### Task 3: ExponentialHistogram and Summary conversion + partial-success threading

This task has two parts that belong together: the conversion arms that produce `rejected_data_points`, and updating the function signature to return that count so the handler (Task 3 step) can build the partial_success envelope. They are reviewed as one unit.

**Files:**
- Modify: `services/ingest-gateway/src/grpc/convert.rs` — replace `_ => {}` with two explicit arms; change return type
- Modify: `services/ingest-gateway/src/grpc/metric.rs` — use new return value for partial_success

**Interfaces:**
- Consumes: nothing from earlier tasks
- Produces:
  - `proto_metrics_to_domain` signature changes from `-> (Vec<domain::MetricSeries>, Vec<domain::MetricPoint>)` to `-> (Vec<domain::MetricSeries>, Vec<domain::MetricPoint>, u64)` where the `u64` is `rejected_data_points`
  - `ExportMetricsServiceResponse { partial_success: Some(...) }` when `rejected_data_points > 0`

- [ ] **Step 1: Write failing tests in convert.rs**

Add to the `#[cfg(test)] mod tests` block in `services/ingest-gateway/src/grpc/convert.rs`, after the existing `proto_metrics_to_domain_uses_stable_series_id_for_same_series` test:

```rust
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
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p ingest-gateway proto_metrics_to_domain 2>&1 | tail -20
```

Expected: compile error — `proto_metrics_to_domain` returns a 2-tuple, tests expect a 3-tuple.

- [ ] **Step 3: Update `proto_metrics_to_domain` signature and add conversion arms**

In `services/ingest-gateway/src/grpc/convert.rs`, make the following changes:

**3a. Update the function signature** — change:

```rust
pub fn proto_metrics_to_domain(
    resource_metrics: &[ResourceMetrics],
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<domain::MetricSeries>, Vec<domain::MetricPoint>) {
    let mut series_list = Vec::new();
    let mut points_list = Vec::new();
    let mut seen_series = HashSet::new();
```

to:

```rust
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
```

**3b. Replace the wildcard arm** — change:

```rust
                    // ExponentialHistogram and Summary: emit as-is with minimal mapping
                    _ => {}
```

to:

```rust
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
```

**3c. Update the return statement** — change:

```rust
    (series_list, points_list)
```

to:

```rust
    (series_list, points_list, rejected_data_points)
```

- [ ] **Step 4: Fix the caller in `metric.rs`**

In `services/ingest-gateway/src/grpc/metric.rs`, update the call site and wire `partial_success`. Replace:

```rust
            let (series, points) = super::convert::proto_metrics_to_domain(
                &inner.resource_metrics,
                tenant_id,
                &environment,
            );
```

with:

```rust
            let (series, points, rejected_data_points) = super::convert::proto_metrics_to_domain(
                &inner.resource_metrics,
                tenant_id,
                &environment,
            );
```

Then replace the final response:

```rust
            Ok(Response::new(ExportMetricsServiceResponse {
                partial_success: None,
            }))
```

with:

```rust
            let partial_success = if rejected_data_points > 0 {
                use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsPartialSuccess;
                Some(ExportMetricsPartialSuccess {
                    rejected_data_points: rejected_data_points as i64,
                    error_message: "ExponentialHistogram bucket detail and Summary quantile values not stored".to_string(),
                })
            } else {
                None
            };
            Ok(Response::new(ExportMetricsServiceResponse { partial_success }))
```

- [ ] **Step 5: Run all ingest-gateway tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -30
```

Expected: all tests pass, including the four new convert tests.

- [ ] **Step 6: Commit**

```bash
git add services/ingest-gateway/src/grpc/convert.rs services/ingest-gateway/src/grpc/metric.rs
git commit -m "feat(ingest-gateway): convert ExponentialHistogram and Summary metrics with partial_success reporting"
```

---

### Task 4: Configurable gRPC max message size

**Files:**
- Modify: `services/ingest-gateway/src/main.rs` — read `INGEST_GRPC_MAX_MESSAGE_BYTES`, pass to gRPC server
- Modify: `services/ingest-gateway/src/grpc/mod.rs` — accept `max_message_bytes` parameter; wire into server

**Interfaces:**
- Consumes: `start_grpc_server` from Task 2 (already modified)
- Produces: `start_grpc_server(state: AppState, port: u16, max_message_bytes: usize)` — updated signature

- [ ] **Step 1: Write failing test**

Add to `services/ingest-gateway/src/main.rs` a new `#[cfg(test)] mod tests` block at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn grpc_max_message_bytes_defaults_to_4mib() {
        std::env::remove_var("INGEST_GRPC_MAX_MESSAGE_BYTES");
        let val: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_194_304);
        assert_eq!(val, 4_194_304);
    }

    #[test]
    fn grpc_max_message_bytes_parses_env_var() {
        std::env::set_var("INGEST_GRPC_MAX_MESSAGE_BYTES", "8388608");
        let val: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_194_304);
        std::env::remove_var("INGEST_GRPC_MAX_MESSAGE_BYTES");
        assert_eq!(val, 8_388_608);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass** (these tests only test the env var parsing logic, which already works):

```bash
cargo test -p ingest-gateway grpc_max_message_bytes 2>&1 | tail -15
```

Expected: both tests pass (they test the `std::env::var` pattern directly).

- [ ] **Step 3: Update `start_grpc_server` to accept `max_message_bytes`**

In `services/ingest-gateway/src/grpc/mod.rs`, replace:

```rust
pub async fn start_grpc_server(state: AppState, port: u16) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let trace_service = trace::OltpTraceService::new(state.clone());
    let log_service = log::OltpLogService::new(state.clone());
    let metric_service = metric::OltpMetricService::new(state.clone());

    tracing::info!(port, "ingest-gateway gRPC listening");

    Server::builder()
        .accept_compressed(CompressionEncoding::Gzip)
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(LogsServiceServer::new(log_service))
        .add_service(MetricsServiceServer::new(metric_service))
        .serve(addr)
        .await?;

    Ok(())
}
```

with:

```rust
pub async fn start_grpc_server(state: AppState, port: u16, max_message_bytes: usize) -> anyhow::Result<()> {
    let addr = format!("0.0.0.0:{}", port).parse()?;

    let trace_service = trace::OltpTraceService::new(state.clone());
    let log_service = log::OltpLogService::new(state.clone());
    let metric_service = metric::OltpMetricService::new(state.clone());

    tracing::info!(port, max_message_bytes, "ingest-gateway gRPC listening");

    Server::builder()
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decode_message_size(max_message_bytes)
        .add_service(TraceServiceServer::new(trace_service))
        .add_service(LogsServiceServer::new(log_service))
        .add_service(MetricsServiceServer::new(metric_service))
        .serve(addr)
        .await?;

    Ok(())
}
```

- [ ] **Step 4: Read env var in `main.rs` and pass it**

In `services/ingest-gateway/src/main.rs`, after the `metric_series_budget` parsing (around line 173), add:

```rust
    let grpc_max_message_bytes: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4_194_304);
```

Then find:

```rust
    let grpc_future = grpc::start_grpc_server(grpc_state, grpc_port);
```

Replace with:

```rust
    let grpc_future = grpc::start_grpc_server(grpc_state, grpc_port, grpc_max_message_bytes);
```

- [ ] **Step 5: Run all ingest-gateway tests**

```bash
cargo test -p ingest-gateway 2>&1 | tail -20
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add services/ingest-gateway/src/grpc/mod.rs services/ingest-gateway/src/main.rs
git commit -m "feat(ingest-gateway): add INGEST_GRPC_MAX_MESSAGE_BYTES config for gRPC max decode size"
```

---

## Self-Review

**Spec coverage:**
- Fix 1 (HTTP zstd): Task 1 ✓
- Fix 2 (gRPC gzip): Task 2 ✓
- Fix 3 (ExponentialHistogram/Summary conversion): Task 3 ✓
- Fix 4 (partial_success envelope): Task 3 (same task, co-located) ✓
- Fix 5 (gRPC max message size): Task 4 ✓

**Placeholder scan:** None found. All code blocks are complete.

**Type consistency:**
- `proto_metrics_to_domain` returns `(Vec<MetricSeries>, Vec<MetricPoint>, u64)` — used as 3-tuple in Task 3 step 4 ✓
- `start_grpc_server(state, port, max_message_bytes)` — updated in both `grpc/mod.rs` (Task 4 step 3) and `main.rs` (Task 4 step 4) ✓
- `rejected_data_points` is `u64` in convert.rs, cast to `i64` in metric.rs before setting on proto field ✓
