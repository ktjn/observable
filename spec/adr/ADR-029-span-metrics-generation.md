# ADR-029: Span Metrics Generation

## Status
Accepted

## Context
As the volume of trace data grows, calculating RED (Rate, Error, Duration) metrics directly from the raw `spans` table becomes computationally expensive and slow for dashboard rendering. OpenTelemetry typically addresses this by deriving metrics from spans at ingestion time.

We need a scalable, consistent way to generate these metrics and store them in our existing metrics system so they can be queried alongside application-emitted metrics.

## Decision
We implemented an **In-Stream Span Metrics Generator** within the `stream-processor` service.

### 1. Architecture Overview
The `stream-processor` consumes spans from the `ingest-topic` (Redpanda), aggregates them in-memory over short windows (60 seconds), and emits OTLP-compliant metrics to the `storage-writer`.

### 2. Key Components

#### A. Span Metrics Aggregator
A module in `stream-processor` (`src/metrics.rs`) that:
- Maintains a thread-safe, windowed state of counters and histograms.
- Dimensions metrics by: `tenant_id`, `service_name`, `operation_name`, `span_kind`, `status_code`, and `environment`.
- Uses the `libs/domain` crate to generate `MetricSeries` and `MetricPoint` types.

#### B. Metric Rules
The following metrics are derived:
1. `span.calls_total` (Sum/Counter): Incremented for every span.
2. `span.duration_ms` (Histogram): Records the duration of every span in milliseconds.
3. `span.errors_total` (Sum/Counter): Incremented when `status_code == ERROR`.

#### C. Deterministic Identity
To ensure that metrics generated from spans are consistent with metrics from other sources, we use the `deterministic_metric_series_id` function from `libs/domain`. This allows the `storage-writer` to deduplicate `MetricSeries` rows correctly in ClickHouse.

### 3. Data Flow
1. **Ingest:** Spans arrive via OTLP and are queued in Redpanda.
2. **Process:** `stream-processor` consumes a span and passes it to the `SpanMetricsAggregator`.
3. **Aggregate:** The aggregator updates its in-memory state.
4. **Emit:** Every 60 seconds, a background task flushes the state:
   - For each unique dimension set, it creates a `MetricSeries` and `MetricPoint`.
   - It sends these to the `storage-writer` via the internal `/internal/metrics` HTTP endpoint.
5. **Storage:** `storage-writer` persists the metrics to ClickHouse.

## Consequences

### Positive
- **Performance:** Dashboards can render RED metrics by querying the small `metric_points` table instead of the massive `spans` table.
- **Consistency:** Derived metrics follow the same schema and identity rules as native OTel metrics.
- **Reduced DB Load:** Aggregating in memory significantly reduces the write IOPS to ClickHouse for metric data.

### Negative
- **Transient State:** If a `stream-processor` instance crashes, the metrics for the current open window (max 60s) are lost. This is considered acceptable for operational metrics as the raw spans are still safe in ClickHouse.
- **Memory Pressure:** High cardinality in span attributes could lead to large memory usage in the aggregator. The current implementation uses a `HashMap`; future iterations may need a cap on the number of active series.

## Alternatives Considered
- **ClickHouse Materialized Views:** Rejected because generating deterministic UUIDs in SQL to match the Rust domain model is difficult, and it adds complexity to the database schema.
- **Dedicated Collector:** Rejected to minimize the number of distinct services in the platform; `stream-processor` already has the necessary context and connectivity.
