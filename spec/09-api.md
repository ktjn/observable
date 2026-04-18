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

#### Field Faceting (Statistics)
- **Capability**: Search endpoints (`GET /v1/logs`, `GET /v1/traces`) must support a `facets` parameter (e.g., `?facets=service_name,log_level,http.status_code`).
- **Response**: The response must include a `facets` object containing the top N (default 10) values and counts for each requested field within the search result set.

#### Live Tail
- **Endpoint**: `GET /v1/logs/tail`
- **Behavior**: Provides a streaming response (Server-Sent Events or chunked JSON) of newly ingested logs.
- **Parameters**: Supports the same filtering parameters as the historical search.

#### Infrastructure Correlation
- **Capability**: Every telemetry record must expose its resource attributes (e.g., `host.name`, `k8s.pod.name`) to enable the UI to link to infrastructure-specific views.

### Resolved Query API Bugs

#### Bug Report: Query API MVP response correctness regressions

**Status:** Resolved in Phase 1 Task 20 (`docs/superpowers/plans/2026-04-17-phase1-internal-mvp.md`)

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

**Status:** Active for trace query endpoints as of Phase 2 slice P2-S1a
(`docs/superpowers/plans/2026-04-18-phases2-8-iteration-plan.md`).

**Affected endpoints:**
- `GET /v1/traces`
- `GET /v1/traces/{trace_id}`

**Required behavior:**
- Query API requests must carry an explicit tenant context before trace handlers execute.
- Missing tenant context must be rejected with `401 Unauthorized`.
- Malformed tenant context must be rejected with `400 Bad Request`.
- Trace lookup and trace search must constrain storage reads by tenant ID.
- Trace responses must not include rows whose `tenant_id` differs from the request tenant. If storage
  returns a mixed-tenant result, the API must fail closed rather than returning partial cross-tenant
  data.

**Closure evidence:**
- Unit tests cover missing, malformed, and valid tenant context extraction.
- Unit tests cover same-tenant trace rows and cross-tenant trace-row rejection.
- The Phase 1 smoke test now sends an explicit `X-Tenant-ID` header for query API calls.

**Required follow-up:**
- Apply the same tenant context and fail-closed validation pattern to log query in P2-S1b.
- Apply the same tenant context and fail-closed validation pattern to metric query in P2-S1c.

**ADR/spec sync:** No ADR update required. This slice enforces the accepted tenant isolation strategy
for an existing query surface and does not change architecture, data model, security model, or
technology choice.
