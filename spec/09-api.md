# API and Extensibility

## 14. API and Extensibility

### Required APIs

- public ingest API
- config API
- query API
- dashboard API
- alert API
- SLO API
- export API
- audit API

### SDKs

- TypeScript/JavaScript
- Go
- Java
- .NET
- Python
- Rust

### Extension Points

- custom parsers
- custom enrichment plugins
- auth provider adapters
- notification adapters
- dashboard widgets
- query macros/functions

### 14.1 Query API Requirements for UI Patterns

The Query API must support the following patterns required by the Frontend (see `spec/05-frontend.md`):

#### Log Context
- **Endpoint**: `GET /v1/logs/{log_id}/context`
- **Behavior**: Returns logs surrounding the specified log line by timestamp, filtered to the same `service_name` and `host.name`.
- **Default Window**: ±1 minute.

#### Log Search
- **Endpoint**: `GET /v1/logs`
- **Parameters**: Supports `service`, `severity`, `trace_id`, `span_id`, `limit`, `facets`,
  `from` (ISO8601), and `to` (ISO8601).
- **Time Range**: Results, totals, and facets are filtered by `timestamp_unix_nano`.
  - If `from` is present: `timestamp_unix_nano >= from`.
  - If `to` is present: `timestamp_unix_nano <= to`.
- **Ordering**: Results are ordered by `timestamp_unix_nano DESC`.

#### Field Faceting (Statistics)
- **Capability**: Search endpoints (`GET /v1/logs`, `GET /v1/traces`) must support a `facets` parameter (e.g., `?facets=service_name,log_level,http.status_code`).
- **Response**: The response must include a `facets` object containing the top N (default 10) values and counts for each requested field within the search result set.

#### Live Tail
- **Endpoint**: `GET /v1/logs/tail`
- **Behavior**: Returns newly ingested logs after a cursor for live-tail clients. The initial
  implementation uses cursor-polled JSON so browser clients can preserve the required tenant header;
  SSE or chunked JSON may replace the transport later without changing the log cursor semantics.
- **Parameters**: Supports `service`, `severity`, `since_unix_nano`, and `limit`.
- **Ordering**: Results are ordered by `timestamp_unix_nano ASC` so clients can append to the tail
  and advance the cursor to the newest returned timestamp.

#### Infrastructure Correlation
- **Capability**: Every telemetry record must expose its resource attributes (e.g., `host.name`, `k8s.pod.name`) to enable the UI to link to infrastructure-specific views.

#### Service Overview Topology
- **Endpoint**: `GET /v1/topology`
- **Capability**: Query endpoints must expose service relationship data derived from traces so the UI can render the Service Overview map without a manually maintained CMDB.
- **Required Fields**: Caller service, callee service, request count, error count/error rate, P95 latency, and the time range used to compute the relationship.
- **Filtering**: Must support environment, tenant, time range (as lookback), and service filters.
- **Behavior**: The current implementation derives relationships by joining spans on `trace_id` and `parent_span_id` within the `QueryPlanner`.

#### Service Detail Summary
- **Capability**: The UI needs a service summary query for the service detail overview and service catalog.
- **Endpoints**:
  - `GET /v1/services/summary` returns the tenant-scoped service catalog summary collection.
  - `GET /v1/services/{service_name}/summary` returns the summary for one service or `404` when the service has no data in the selected lookback.
- **Required Fields**: Request rate, error rate, latency percentiles, active alert count, current SLO state, latest deployment marker, and links or identifiers for related logs, metrics, traces, and infrastructure entities.
- **Current Response Fields**: `service_name`, `request_rate`, `error_rate`, `p95_latency_ms`, `health_state`, `active_alert_count`, and `latest_deployment`. `active_alert_count` and `latest_deployment` are now populated: `active_alert_count` counts active `slo_burn_rate` alerts linked via `slo_definitions.service_name` (alert types not tied to an SLO are not counted, since `alert_rules` has no `service_name` column — a known limitation), and `latest_deployment` is the most recent `deployment_markers.service_version` for that service.
- **Filtering**: Must support project, environment, tenant, service, and time range filters. The current implementation supports tenant through `X-Tenant-ID`, `environment`, service path parameter for the single-service endpoint, and `lookback_minutes`.

#### Infrastructure Views
- **Capability**: Query endpoints must support infrastructure inventory and detail views for hosts, Kubernetes clusters, namespaces, pods, and containers when those resource attributes or catalog entities exist.
- **Required Fields**: Entity identity, entity type, health state, CPU, memory, disk, network, restart count where applicable, recent log/error rate, related service names, and last-seen timestamp.
- **Filtering**: Must support project, environment, tenant, entity type, entity ID/name, related service, and time range filters.

#### Dashboard Promotion
- **Endpoints**:
  - `GET /v1/dashboards` returns tenant-scoped saved dashboards.
  - `GET /v1/dashboards/{dashboard_id}` returns one tenant-scoped dashboard and all panels.
  - `POST /v1/dashboards` creates one dashboard with one or more promoted or authored panels.
  - `PUT /v1/dashboards/{dashboard_id}` replaces the dashboard name and panel set for layout/content edits.
  - `GET /v1/dashboards/{dashboard_id}/export` exports a portable dashboard artifact.
  - `POST /v1/dashboards/import` imports a portable dashboard artifact.
- **Behavior**: A dashboard is a set of query and text panels. Query panels persist the selected
  query kind, service filter, structured filter metadata, optional NLQ/raw-IR query text, persisted
  grid layout, and panel time-range behavior. Text panels persist explanation content and layout.
- **Current Response Fields**: `dashboard_id`, `name`, `created_at`, and `panels[]` with
  `panel_id`, `title`, `panel_kind`, `query_kind`, `service`, `preset`, `filters`, `query_text`,
  `content`, `layout`, and `time_range`.
- **Current Panel Types**: `panel_kind` values are `query` and `text`. For query panels, `logs`,
  `traces`, and `metrics` are valid `query_kind` values. Text panels do not require `query_kind`.
- **Time Range**: `time_range.mode = "global"` follows the UI global date selector.
  `time_range.mode = "preset"` uses the panel's relative preset. `time_range.mode = "absolute"`
  uses saved `from_ms` and `to_ms` values.
- **Import/Export**: Export schema version `2` includes text/query panel fields, layout, and
  time-range behavior. Version `1` imports remain supported by converting legacy panels to query
  panels with default layout and global or preset time behavior.
- **Out of Scope**: Drag-and-drop layout editing and CI dashboard linting are handled by later
  dashboard-builder slices.

#### Metric Query Readback
- **Endpoints**:
  - `GET /v1/metrics` returns tenant-scoped metric catalog entries grouped by metric identity
    (`metric_name`, service, environment, type, unit, monotonicity, and temporality), with
    `series_count` showing how many label/resource-specific series back each entry. Supports
    optional `service`.
  - `GET /v1/metrics/points` returns tenant-scoped points aggregated across the label/resource
    series for one grouped metric identity. Numeric sum metrics are summed per timestamp; gauges
    are averaged per timestamp.
  - `GET /v1/metrics/{series_id}` returns tenant-scoped points for one metric series, ordered by `time_unix_nano ASC`.
- **Ingest compatibility**: `POST /v1/metrics` accepts OTLP/HTTP JSON gauges, sums, and explicit histograms. The gRPC MetricsService maps those families into the same internal model and also accepts exponential histograms and summaries.
- **Series identity**: Metric series IDs are deterministic for a stable tenant, metric name, metric type, point attributes, resource attributes, monotonicity, temporality, service, and environment. Repeated exports for the same series must append points to the same series ID rather than creating a new random series per point.
- **Reduced-detail families**: OTLP exponential histograms and summaries retain count and optional sum in the internal metric model. Exponential-histogram bucket detail and summary quantile values are not stored; when those details are present, the gRPC MetricsService reports the affected data-point count through OTLP partial success.
- **Verification**: The smoke test must prove readback by posting `smoke.counter`, waiting for the metric catalog entry through `GET /v1/metrics?service=...`, and then waiting for points through `GET /v1/metrics/points` or the raw-series compatibility endpoint `GET /v1/metrics/{series_id}`.

### Resolved Query API Bugs

#### Bug Report: Query API MVP response correctness regressions

**Status:** Resolved in Phase 1 (internal MVP)

**Affected endpoints:**
- `GET /v1/traces`
- `GET /v1/logs`
- `GET /v1/metrics/:series_id`

**Resolved issues:**
1. `GET /v1/traces` no longer depends on a `SELECT DISTINCT trace_id ... ORDER BY start_time_unix_nano DESC`
   pattern. Trace listing must order by an explicit aggregate such as the latest span timestamp.
2. `GET /v1/traces` and `GET /v1/logs` must report `total` independently of page size.
3. `GET /v1/metrics/:series_id` must preserve absent histogram arrays as absent values rather than
   serializing non-histogram points as present-but-empty histogram payloads.

**Expected behavior:**
- Trace list ordering must be based on a deterministic "latest span in trace" value.
- Response `total` fields must report the total match count independently of page size.
- Metric point serialization must preserve optional histogram fields exactly.

**Previous impact:**
- Trace explorer could show stale or unstable ordering.
- Clients could not implement correct pagination or result-count UX.
- Metric consumers could misclassify gauge and sum points as histogram-like records.

**Closure evidence:**
- Phase 1 Task 20 records the query ordering/count fixes, metric point serialization fix, discovery
  endpoints, and smoke-test verification.

**Required follow-up:**
- Keep these response-shape guarantees covered by regression tests when tenant isolation, RBAC, and
  retention filters are added in Phase 2.

**ADR/spec sync:** No ADR update required. This report documents implementation bugs against the
existing API contract rather than a change to architecture, data model, security model, or
technology choice.

### Query Tenant Context Contract

**Status:** Active for trace, log, and metric query endpoints. The completed Phase 2 slices P2-S1a
through P2-S1c are done; current follow-on backlog tracking lives in `ROADMAP.md`.

**Affected endpoints:**
- `GET /v1/traces`
- `GET /v1/traces/{trace_id}`
- `GET /v1/logs`
- `GET /v1/metrics`
- `GET /v1/metrics/{series_id}`

**Required behavior:**
- Query API requests must carry an explicit tenant context before handlers execute.
- Missing tenant context must be rejected with `401 Unauthorized`.
- Malformed tenant context must be rejected with `400 Bad Request`.
- Trace, log lookup, and log search must constrain storage reads by tenant ID.
- Responses must not include rows whose `tenant_id` differs from the request tenant. If storage
  returns a mixed-tenant result, the API must fail closed rather than returning partial cross-tenant
  data.

**Closure evidence:**
- Unit tests cover missing, malformed, and valid tenant context extraction.
- Unit tests cover same-tenant trace rows and cross-tenant trace-row rejection (P2-S1a).
- Unit tests cover same-tenant log rows, cross-tenant log-row rejection, and empty result (P2-S1b).
- Unit tests cover same-tenant metric series/point rows, cross-tenant metric row rejection, and empty metric results (P2-S1c).
- The Phase 1 smoke test now sends an explicit `X-Tenant-ID` header for query API calls.

**Required follow-up:**
- Keep tenant fail-closed validation in place as metric query filters expand beyond service and series ID.

**ADR/spec sync:** No ADR update required. This slice enforces the accepted tenant isolation strategy
for an existing query surface and does not change architecture, data model, security model, or
technology choice.

### 14.2 Deployment Marker API

The Query API and Ingest API must support the deployment marker schema and logic defined in `spec/18-deployment-markers.md`.

#### Ingest Deployment Marker
- **Endpoints**: `POST /v1/deployments`, `PATCH /v1/deployments/{id}`
- **Port**: Platform API port (4321) — separate from OTLP ports (4317/4318). See ADR-023.
- **Behavior**: Enables lifecycle tracking of releases (start, finish, fail, rollback).
- **Authentication**: Requires `Member` or higher project-level role.

#### List Deployment Markers
- **Endpoint**: `GET /v1/deployments`
- **Behavior**: Returns deployment events for UI timeline overlays, filterable by service and environment.

#### Alert Rule Management
- **Endpoints**:
  - `GET /v1/alerts/rules`
  - `POST /v1/alerts/rules`
  - `PATCH /v1/alerts/rules/{rule_id}/silence`
- **Behavior**: Enables management of threshold-based alert rules.
- **Filtering**: `GET` returns rules for the authenticated tenant.
- **Auth**: Requires `Member` role for create/silence.

### 14.3 Saved Views API

Endpoints: `GET/POST /v1/saved-views`, `GET/PUT/DELETE /v1/saved-views/{id}`,
`GET/POST /v1/saved-views/{id}/grants`, `DELETE /v1/saved-views/{id}/grants/{user_id}`.

Behavior: Mirrors the Dashboards API's visibility/grant model exactly —
`visibility` is `private` (default) or `public`; grants carry `owner`/`editor`/`viewer`
relations. `signal_kind` is currently restricted to `logs` (widens to `traces`/`metrics`
in follow-up slices). `config` is an opaque JSON object interpreted only by the
frontend — the backend validates it is a JSON object and stores/returns it verbatim.

Auth: Requires `require_tenant` middleware (tenant-scoped). API-key callers see all
tenant saved views; session users see public views plus views they hold a grant on,
identical to dashboards.

### 14.4 Prometheus Remote Write Ingest

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
