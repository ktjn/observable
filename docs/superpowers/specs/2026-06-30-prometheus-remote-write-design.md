# Prometheus Remote Write Receiver — Design

**Date:** 2026-06-30
**Status:** Approved
**ADR:** ADR-017-prometheus-remote-write.md
**Roadmap:** `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §3 (Tier 1, top item)

---

## 1. Goal

Accept Prometheus `remote_write` v1 payloads at the ingest-gateway so operators can point an
existing Prometheus agent stack at Observable without any OTel Collector translation hop.
Translated metrics enter the same `MetricSeries` / `MetricPoint` pipeline as OTLP metrics.

---

## 2. Endpoint

```
POST http://host:4321/api/v1/write
Authorization: Bearer <api-key>
Content-Type: application/x-protobuf
```

- Hosted on the **platform port (4321)** alongside `POST /v1/deployments` and
  `POST /v1/events/changes`. The OTLP port (4318) remains strictly OTLP per the existing router
  comment and ADR-001.
- Auth: existing `auth_middleware` — Bearer API key → `TenantContext` (tenant_id + environment).
  `X-Tenant-ID` header is ignored; tenant comes from the key, same as every other ingest path.
- Success response: **`204 No Content`** (Prometheus expects 204, not 200).
- Remote_write v2 (`application/x.prometheus.remote+proto`) is out of scope — return `415`.

---

## 3. Module Structure

```
services/ingest-gateway/src/prometheus_rw/
  mod.rs      — Axum handler
  proto.rs    — Hand-rolled prost Message structs for WriteRequest
  convert.rs  — WriteRequest → (Vec<MetricSeries>, Vec<MetricPoint>)
```

New dependency in `services/ingest-gateway/Cargo.toml`:

```toml
snap = "1"   # snappy decompression
```

`prost` is already a workspace dependency.

---

## 4. Proto Types (`proto.rs`)

Hand-rolled prost structs — the remote_write v1 schema is ~5 message types and has not changed
meaningfully since 2019. No `build.rs` or vendored `.proto` files needed.

```rust
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

#[derive(prost::Message)]
pub struct Label {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(string, tag = "2")]
    pub value: String,
}

#[derive(prost::Message)]
pub struct Sample {
    #[prost(double, tag = "1")]
    pub value: f64,
    #[prost(int64, tag = "2")]
    pub timestamp: i64,  // milliseconds since epoch
}
```

---

## 5. Decoding Sequence (handler, `mod.rs`)

1. Assert `Content-Type: application/x-protobuf` — return `415` otherwise.
2. Snappy-decompress body bytes via `snap::raw::Decoder`.
3. `prost::Message::decode()` → `WriteRequest`.
4. Any failure in steps 2–3 → `400 Bad Request`.
5. Empty `WriteRequest` (zero timeseries) → `204` immediately, nothing published.

---

## 6. Label-to-Attribute Translation (`convert.rs`)

Public function signature:

```rust
pub fn write_request_to_metrics(
    req: WriteRequest,
    tenant_id: Uuid,
    environment: &str,
) -> (Vec<MetricSeries>, Vec<MetricPoint>)
```

### 6.1 Label mapping (per `TimeSeries`)

| Prometheus label | Destination |
|---|---|
| `__name__` | `MetricSeries.metric_name` |
| `job` | `MetricSeries.service_name` (fallback: `"unknown"`) |
| `instance` | `resource_attributes["host.name"]` |
| `observable.service_name` | overrides `service_name` if present |
| all other labels | `MetricSeries.attributes` (string key/value) |

### 6.2 Metric type inference (from `__name__` suffix)

| Suffix | `MetricType` | Notes |
|---|---|---|
| `_total` | `Sum` | `is_monotonic = true`, `aggregation_temporality = Cumulative` |
| `_created` | — | Skipped entirely (internal Prometheus bookkeeping) |
| `_bucket` / `_count` / `_sum` | `Histogram` | Grouped by base name + non-`le` label set (see §6.3) |
| anything else | `Gauge` | |

### 6.3 Histogram reconstruction

Series whose `__name__` ends in `_bucket`, `_count`, or `_sum` are grouped by:
- base name (strip the suffix)
- label set excluding `__name__` and `le`

Per group and per timestamp, one `MetricPoint` is emitted with:
- `histogram_explicit_bounds`: sorted `le` label values from `_bucket` samples (excluding `+Inf`)
- `histogram_bucket_counts`: corresponding sample values (cumulative, as Prometheus sends them)
- `histogram_count`: value from the `_count` sample at the same timestamp (if present)
- `histogram_sum`: value from the `_sum` sample at the same timestamp (if present)

If a bucket group has no `+Inf` bucket it cannot be completed as a histogram; those samples are
emitted as individual Gauge points rather than silently dropped.

### 6.4 Common fields on every series

- `resource_attributes["observable.ingest_source"] = "prometheus_remote_write"` (ADR-017)
- `metric_series_id`: `domain::deterministic_metric_series_id(&series)` — same as OTLP path
- `time_unix_nano`: `Sample.timestamp (ms) * 1_000_000`

---

## 7. Handler Flow (`mod.rs`)

Follows the same shape as `http-json/metrics.rs::export_metrics`:

```
content-type guard
→ snap decode
→ prost decode
→ convert::write_request_to_metrics
→ metric_rate_limiter.check_key      (429 on exceeded)
→ metric_cardinality.observe
→ producer.publish(EnvelopePayload::Metrics { series, points })
→ 204 No Content
```

### Error responses

| Condition | Status |
|---|---|
| Wrong content-type | `415 Unsupported Media Type` |
| Snappy decode failure | `400 Bad Request` |
| Proto decode failure | `400 Bad Request` |
| Rate limit exceeded | `429 Too Many Requests` + `Retry-After: 1` |
| Queue publish failure | `500 Internal Server Error` |

---

## 8. Router Change

`build_platform_router` in `src/http-json/mod.rs` gets one new route inside the authenticated
sub-router:

```rust
.route("/api/v1/write", post(prometheus_rw::write))
```

`main.rs` gets `mod prometheus_rw;`.

---

## 9. Tests

### Unit tests (`convert.rs`)

- Gauge series → `MetricType::Gauge`, labels → `attributes`
- `_total` suffix → `MetricType::Sum`, `is_monotonic = true`, `aggregation_temporality = Cumulative`
- `job` → `service_name`; `instance` → `resource_attributes["host.name"]`
- `observable.service_name` label overrides `job`
- Histogram grouping: `_bucket` + `_count` + `_sum` → one `MetricSeries` + one `MetricPoint`
  with correct `histogram_bucket_counts`, `histogram_explicit_bounds`, `histogram_count`, `histogram_sum`
- `_created` suffix → skipped (zero output)
- Timestamp ms → ns conversion (`* 1_000_000`)
- `observable.ingest_source = "prometheus_remote_write"` on every series

### Integration tests (`mod.rs`, using `axum-test` + `AppState::with_stub_auth`)

- Valid snappy-proto body → `204`
- Wrong content-type → `415`
- Malformed snappy body → `400`
- Missing auth → `401`
- Rate limit exceeded → `429` + `Retry-After: 1`
- Empty `WriteRequest` → `204`

No Testcontainers test needed — the queue publish path is already covered by existing OTLP
metrics integration tests.

---

## 10. Out of Scope

- Prometheus remote_write v2 (`application/x.prometheus.remote+proto`)
- PromQL query facade (separate Tier 2 item)
- Prometheus Alert Rule Importer (depends on this feature; separate Tier 2 item)
- `X-Tenant-ID` header tenant routing

---

## 11. ADR / Spec Sync

- ADR-017 is already Accepted. Add the implementation section to `spec/09-api.md` in the same PR
  (document the endpoint URL, auth method, content-type, and success/error codes).
- No new ADR needed.
