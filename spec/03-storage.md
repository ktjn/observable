# Storage and Query Engine

## 5. Storage Strategy

### 5.1 Recommended Storage Pattern

Use polyglot storage — not one engine for all workloads.

**Logs and traces**

ClickHouse is a strong fit for observability workloads. It is explicitly positioned for logs and traces with guidance on schema design and data management for large-scale observability datasets.

**Metrics**

Use ClickHouse for metrics in Phase 1 and until a concrete performance or cardinality constraint justifies a dedicated TSDB. ClickHouse supports all required metric workload characteristics:
- high ingest throughput
- columnar compression
- rollups/downsampling via materialized views
- histogram and exponential histogram storage
- exemplar support
- label filtering via WHERE clauses on attribute columns

**Revisit condition:** If Phase 2 or Phase 3 cardinality testing reveals that ClickHouse cannot meet the P50 < 1 s query target under production-representative label cardinality, open a new ADR to evaluate a dedicated TSDB (e.g., VictoriaMetrics). The query facade abstracts storage engines from clients, so a later migration is contained.

**Profiles**

Separate profile storage/indexing is acceptable initially; converge later behind a unified API. OTel profiles are still emerging, so design the domain model now but keep implementation modular.

**Object storage**

Use for:
- raw chunks
- long-term archives
- symbol files
- sourcemaps
- profile blobs
- replay artifacts
- query spill

### 5.2 Logical Data Model

**Common dimensions**

| Field | Description |
|-------|-------------|
| `tenant_id` | |
| `account_id` | |
| `project_id` | |
| `environment` | |
| `service_name` | |
| `service_namespace` | |
| `service_version` | |
| `deployment_id` | |
| `region` | |
| `cloud_provider` | |
| `cluster` | |
| `namespace` | |
| `workload` | |
| `host_id` | |
| `container_id` | |
| `trace_id` | |
| `span_id` | |
| `session_id` | |
| `user_hash` | |
| `tags` | map |

**Metrics schema**
- `metric_name`
- `type` (`gauge`, `sum`, `histogram`, `exponential_histogram`, `summary`)
- `timestamp`
- `start_timestamp` when supplied by OTel cumulative/delta streams
- numeric value for gauges and sums
- explicit histogram buckets
- exponential histogram buckets
- summary count/sum/quantiles for compatibility imports
- `dimensions` / metric attributes
- `resource_attrs`
- instrumentation scope name/version
- aggregation temporality and monotonicity where applicable
- exemplars with optional `trace_id` / `span_id` refs

**Logs schema**
- `timestamp`
- `severity`
- `body`
- `attributes`
- `resource_attrs`
- `trace_id` / `span_id` refs
- `fingerprint`
- `parsed_fields`

Logs and spans are correlated, not nested. Exact span-log joins require `tenant_id + trace_id + span_id`; trace-level log joins may use `tenant_id + trace_id` when the log lacks a span id.

**Traces schema**
- `trace_id`
- `span_id`
- `parent_span_id`
- `start` / `end`
- `duration`
- `status`
- `kind`
- `attrs`
- `events`
- `links`
- `resource_attrs`

Spans are the source of truth for trace waterfall reconstruction. Trace rows are materialized rollups, and span events remain part of the span payload unless explicitly duplicated into logs by a transformation.

**Profiles schema**
- `profile_id`
- `sample_type`
- `period`
- `stack_frames`
- `symbols`
- `labels`
- `service` / `version` / `build` refs

### 5.3 Retention Tiers

| Tier | Duration | Characteristics |
|------|----------|-----------------|
| hot | 3–14 days | full fidelity |
| warm | 15–60 days | indexed + partial rollups |
| cold | 2–12 months | compressed / object-backed |
| archive | compliance | export, restore-on-demand |

---

## 6. Query Engine and Compute

A Rust-native query/middle tier is a strong fit. Apache DataFusion is an extensible Rust query engine built on Arrow, which aligns well with an observability platform needing custom execution and columnar processing. Arrow's ecosystem makes it a credible foundation for data interchange and in-memory execution.

### 6.1 Recommendation

- Write control services and query orchestration in Rust.
- Use Arrow as in-memory format.
- Use DataFusion for:
  - federated query planning
  - custom logical operators
  - query rewriting
  - cost-aware pushdown
  - cross-signal joins

### 6.2 Custom Operators

Implement:
- trace waterfall reconstruction
- service graph rollup
- percentile/histogram operators
- SLO burn-rate windows
- anomaly operators
- cardinality estimation
- span-to-log correlation joins
