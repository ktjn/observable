# OTLP Compatibility Fixes — Design

## Goal

Fix five wire-protocol gaps in `ingest-gateway` that cause silent data loss or protocol errors when clients send gzip-compressed gRPC, zstd-compressed HTTP, ExponentialHistogram/Summary metric types, or large payloads.

## Architecture

All changes are confined to `services/ingest-gateway`. No other service is modified. The fixes split across three layers: decompression (HTTP and gRPC), metric type conversion, and response envelope.

```
OTel SDK / collector
      │
      ├─ HTTP POST /v1/metrics  ← Fix 1: add zstd decompression
      │
      └─ gRPC ExportMetrics     ← Fix 2: accept gzip-compressed requests
                                   Fix 5: configurable max message size
              │
              └─ convert.rs     ← Fix 3: ExponentialHistogram + Summary → MetricPoint
                                   Fix 4: partial_success response when data dropped
```

## Fix 1 — HTTP zstd Decompression

**File:** `services/ingest-gateway/src/http-json/mod.rs`

`decode_request_body` currently handles `Content-Encoding: ""`, `"identity"`, and `"gzip"` (via flate2), returning HTTP 415 for anything else. Adding a `"zstd"` arm:

- Collect body bytes into a `Vec<u8>`.
- Decompress with `zstd::decode_all(std::io::Cursor::new(&bytes))`.
- Return HTTP 400 with a plain-text error on decompression failure.
- Return 415 for any other encoding value (unchanged).

**New dependency:** `zstd = "0.13"` in `services/ingest-gateway/Cargo.toml`.

## Fix 2 — gRPC Compression

**Files:** root `Cargo.toml` (workspace), `services/ingest-gateway/src/grpc/mod.rs`

Tonic 0.14 supports gzip via a Cargo feature. Currently the workspace entry is `tonic = "0.14"` with no features. Adding `features = ["gzip"]` unlocks `tonic::codec::CompressionEncoding::Gzip`.

In `grpc/mod.rs`, chain on the `Server::builder()`:

```rust
.accept_compressed(CompressionEncoding::Gzip)
```

This makes tonic decompress gzip-compressed request bodies transparently before the handler sees them.

## Fix 3 — ExponentialHistogram and Summary Metric Types

**File:** `services/ingest-gateway/src/grpc/convert.rs`

The current wildcard arm `_ => {}` silently drops all ExponentialHistogram and Summary data points. The domain types `MetricType::ExponentialHistogram` and `MetricType::Summary` already exist in `libs/domain/src/metric.rs`. `MetricPoint` has `histogram_count: Option<u64>` and `histogram_sum: Option<f64>` that can carry meaningful data.

Replace `_ => {}` with two explicit arms:

**ExponentialHistogram arm:**
- Set `metric_type = MetricType::ExponentialHistogram`
- Map `data_point.count → histogram_count`
- Map `data_point.sum → histogram_sum`
- Bucket arrays (positive/negative buckets, scale, offset, zero_count) are not stored — no matching fields in `MetricPoint`
- Increment `rejected_data_points` by 1 per data point that has non-empty positive or negative buckets (the OTel spec counts rejected data points, not sub-fields)
- Emit `tracing::debug!` once per batch (not per point): `"ExponentialHistogram: storing count+sum only, bucket detail dropped"`

**Summary arm:**
- Set `metric_type = MetricType::Summary`
- Map `data_point.count → histogram_count`
- Map `data_point.sum → histogram_sum`
- Quantile values not stored — no matching field in `MetricPoint`
- Increment `rejected_data_points` by 1 per data point that has non-empty quantile values
- Emit `tracing::debug!` once per batch: `"Summary: storing count+sum only, quantile values dropped"`

## Fix 4 — Partial-Success Response Envelope

**File:** `services/ingest-gateway/src/grpc/mod.rs`

The metrics gRPC handler currently returns `ExportMetricsServiceResponse { partial_success: None }` unconditionally. After conversion, if `rejected_data_points > 0`:

```rust
ExportMetricsServiceResponse {
    partial_success: Some(ExportMetricsPartialSuccess {
        rejected_data_points: rejected_data_points as i64,
        error_message: "ExponentialHistogram bucket detail and Summary quantile values not stored".into(),
    }),
}
```

On zero rejections, `partial_success: None` (unchanged).

`rejected_data_points` is accumulated by the conversion layer (Fix 3) and threaded back to the handler.

## Fix 5 — Configurable gRPC Max Message Size

**Files:** `services/ingest-gateway/src/config.rs`, `services/ingest-gateway/src/main.rs`

Tonic defaults to 4 MiB max decode message size. Large batches from high-throughput collectors exceed this and are silently rejected at the transport layer.

Add to the config struct:

```rust
pub grpc_max_message_bytes: usize,  // default: 4_194_304 (4 MiB)
```

Read from env var `INGEST_GRPC_MAX_MESSAGE_BYTES`. If unset, default to `4_194_304` (preserves existing behaviour).

Wire into server builder:

```rust
.max_decode_message_size(config.grpc_max_message_bytes)
```

## Data Model Impact

No schema changes. No ClickHouse migrations. `MetricPoint` stores `histogram_count` and `histogram_sum` for ExponentialHistogram and Summary, same as it does for explicit Histograms. Bucket and quantile detail is intentionally dropped with transparency via partial_success.

Future work: add `exp_histogram_scale`, `exp_histogram_positive_*`, etc. fields to `MetricPoint` + schema migration when UI rendering for exponential histograms is built.

## Testing

| Fix | Test location | What it covers |
|-----|--------------|----------------|
| 1 | `src/http-json/mod.rs` unit tests | zstd round-trip; corrupt bytes → 400 |
| 2 | existing gRPC integration tests | server still builds; no regression |
| 3 | `src/grpc/convert.rs` unit tests | ExponentialHistogram → MetricPoint fields; Summary → MetricPoint fields; rejected_data_points counter |
| 4 | gRPC handler unit test | ExponentialHistogram in request → partial_success.rejected_data_points > 0 |
| 5 | config unit tests | INGEST_GRPC_MAX_MESSAGE_BYTES parses correctly; default = 4 MiB |

All existing `cargo test -p ingest-gateway` tests must continue to pass.

## Global Constraints

- All changes confined to `services/ingest-gateway` (and root `Cargo.toml` for tonic feature flag)
- No new domain types; no ClickHouse schema changes
- `zstd = "0.13"` — use this exact version
- tonic gzip feature: `features = ["gzip"]` on the workspace `tonic` entry
- Default `INGEST_GRPC_MAX_MESSAGE_BYTES = 4_194_304` (must not change existing behaviour when env var is unset)
- `rejected_data_points` field type in proto is `int64` — cast from `usize` before setting
- Partial-success error message verbatim: `"ExponentialHistogram bucket detail and Summary quantile values not stored"`
- Debug log messages verbatim: `"ExponentialHistogram: storing count+sum only, bucket detail dropped"` and `"Summary: storing count+sum only, quantile values dropped"`
