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

The authoritative common dimensions reference (OTel attribute mappings, descriptions, and injection rules) is in `spec/14-domain-model.md §7`. The fields below are the storage-layer summary; refer to the domain model for semantics.

| Field | Description |
|-------|-------------|
| `tenant_id` | Top-level isolation key |
| `account_id` | Billing entity |
| `org_id` | Organization grouping layer; injected at ingest from project-to-org mapping |
| `project_id` | Project scope |
| `environment` | Named deployment stage |
| `service_name` | Service identifier |
| `service_namespace` | Optional service grouping |
| `service_version` | Deployed version |
| `deployment_id` | Active deployment at time of signal |
| `region` | Cloud or datacenter region |
| `cloud_provider` | e.g. `aws`, `gcp`, `azure` |
| `cluster` | Kubernetes cluster |
| `namespace` | Kubernetes namespace |
| `workload` | Runtime instance identifier |
| `host_id` | Physical/virtual machine ID |
| `container_id` | Container instance ID |
| `trace_id` | Distributed trace identifier |
| `span_id` | Span within trace |
| `session_id` | RUM session identifier; present on browser/mobile SDK signals only |
| `user_hash` | One-way hash of end-user identity; present on browser/mobile SDK signals only |
| `tags` | Operator-defined key-value labels from the control plane; distinct from OTel signal `attributes` |

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

Profile blobs are stored in object storage. Indexing uses a lightweight ClickHouse metadata table (profile_id, tenant_id, service_name, sample_type, start_time, end_time, environment, host_id) that enables time-range and service-scoped queries without scanning blob content. Full stack frame data is fetched from object storage on demand. Symbol resolution (source file + line number mapping) uses a separate symbol table also in object storage, keyed by `service_version` + `build_id`.

### 5.3 Retention Tiers

| Tier | Duration | Characteristics |
|------|----------|-----------------|
| hot | 3–14 days | full fidelity |
| warm | 15–60 days | indexed + partial rollups |
| cold | 2–12 months | compressed / object-backed |
| archive | compliance | export, restore-on-demand |

### 5.4 Schema Registry

The Schema Registry is a control plane service that tracks and versions the schemas of telemetry types and custom instrumentation. It is distinct from `spec/14-domain-model.md`, which is the static design-time authoritative model. The Schema Registry is the runtime authority.

**Responsibilities:**
- Stores the canonical field definitions for each telemetry type (Span, LogRecord, MetricSeries, MetricPoint, ProfileSample, Event, SyntheticCheck), versioned against the OTel specification release that introduced each field
- Discovers and indexes custom attributes observed at ingest time (schema-on-write); tracks cardinality and last-seen timestamps per attribute key
- Exposes a gRPC/HTTP API used by the query facade to resolve field names, types, and cardinality hints for query autocomplete and cost estimation
- Enforces the `ADR-013` schema governance tiers: standard (strictly enforced), high-velocity (auto-indexed), low-velocity (schema-on-read)
- Versioned: schema changes increment a monotonic version; breaking changes require a migration plan

**Storage:** Schema Registry metadata is stored in the relational control plane store (PostgreSQL). It is not stored in ClickHouse.

**API surface:**
- `GET /schemas/{signal_type}` — current schema for a signal type
- `GET /schemas/{signal_type}/attributes` — discovered attributes with cardinality and index status
- `POST /schemas/{signal_type}/attributes/{key}/index` — promote a schema-on-read attribute to schema-on-write (operator action)
- `GET /schemas/versions` — schema version history

#### 5.4.1 Semantic Annotations

The Schema Registry tracks *structural* metadata — field names, types, cardinality hints. To enable
reliable LLM-assisted query generation (see [spec/08 §13.1](08-ai-ml.md)), it must also carry
*semantic* metadata: operator-authored annotations that describe the business meaning of each field
or attribute key, independent of its OTel type.

Semantic annotations are optional, operator-maintained overlays on top of schema entries. They do
not affect query execution or indexing. They are exposed via the Schema Registry API and consumed
by the NL query layer to ground generated queries.

**Semantic annotation fields (per attribute key or metric name):**

| Field | Type | Description |
|---|---|---|
| `display_name` | string | Human-readable label for use in UI and LLM narration (e.g. `"Checkout Revenue (EUR)"`) |
| `business_description` | string | Free-text explanation of what this field represents in business terms |
| `owner_team` | string | Team or contact responsible for this signal (e.g. `"payments-team"`) |
| `interpretation_rule` | enum | How to interpret the direction of change: `higher_is_worse`, `higher_is_better`, `directional`, `contextual` |
| `effective_sample_rate` | float | Approximate fraction of events captured (e.g. `0.05` for 5% tail sampling). Used by LLM to qualify count estimates. |
| `known_derivations` | string[] | Named derived metrics or views computed from this field (e.g. `["p99_latency", "error_rate"]`) |
| `not_for_billing` | bool | Explicit marker that this field is approximate and must not be used for billing or contractual SLA evidence |

**Metric-type extensions (required by MCP server for time-series SQL generation):**

These additional fields are required when the signal is a metric, so the MCP translation layer
(see [spec/08 §13.1](08-ai-ml.md) and [ADR-021](adr/ADR-021-nl-query-layer.md)) can select
the correct SQL pattern:

| Field | Type | Description |
|---|---|---|
| `metric_type` | enum | `counter`, `gauge`, `histogram`, `summary` — determines whether counter-reset detection and `rate`/`irate` patterns apply |
| `timestamp_column` | string | Name of the timestamp column for this metric's table (e.g. `"timestamp_unix_nano"`) |
| `unit` | string | Canonical unit of measurement (e.g. `"ms"`, `"bytes"`, `"req"`, `"1"`) used in VisualizationFrame output |
| `recommended_downsampling` | string | Default time-bucket resolution for dashboards and NLQ (e.g. `"1m"`, `"5m"`, `"1h"`) |

**API additions for semantic annotations:**
- `GET /schemas/{signal_type}/attributes/{key}/annotations` — get semantic annotations for a field
- `PUT /schemas/{signal_type}/attributes/{key}/annotations` — create or replace annotations (operator role required)
- `PATCH /schemas/{signal_type}/attributes/{key}/annotations` — partial update
- `DELETE /schemas/{signal_type}/attributes/{key}/annotations` — remove annotations

**Storage:** Semantic annotations are stored in PostgreSQL alongside the Schema Registry entries.
They are versioned with the same monotonic version counter as structural schema changes.

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
