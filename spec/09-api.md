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

### Known Query API Bugs

#### Bug Report: Query API MVP response correctness regressions

**Status:** Open

**Affected endpoints:**
- `GET /v1/traces`
- `GET /v1/logs`
- `GET /v1/metrics/:series_id`

**Observed issues:**
1. `GET /v1/traces` currently uses a `SELECT DISTINCT trace_id ... ORDER BY start_time_unix_nano DESC`
   pattern. In ClickHouse, `DISTINCT` is applied before `ORDER BY`, so the returned trace order is
   not guaranteed to reflect the most recent span for each trace.
2. `GET /v1/traces` and `GET /v1/logs` currently set `total` to the number of rows returned after
   `LIMIT`, not the total number of matching results. This breaks pagination and any UI that needs
   "showing N of M" semantics.
3. `GET /v1/metrics/:series_id` currently returns `histogram_bucket_counts` and
   `histogram_explicit_bounds` as present-but-empty arrays for non-histogram points because the
   storage-row conversion collapses `None` into `[]` and restores it as `Some(vec![])`. This
   changes the metric point shape and loses the distinction between non-histogram data and an empty
   histogram payload.

**Expected behavior:**
- Trace list ordering must be based on a deterministic "latest span in trace" value.
- Response `total` fields must report the total match count independently of page size.
- Metric point serialization must preserve optional histogram fields exactly.

**Impact:**
- Trace explorer can show stale or unstable ordering.
- Clients cannot implement correct pagination or result-count UX.
- Metric consumers can misclassify gauge and sum points as histogram-like records.

**Required follow-up:**
- Replace the trace listing query with a grouped or subquery-based form that orders by an explicit
  aggregate such as `max(start_time_unix_nano)`.
- Add a count query or equivalent pagination metadata for list endpoints.
- Preserve `Option<Vec<_>>` semantics across metric point storage conversions.

**ADR/spec sync:** No ADR update required. This report documents implementation bugs against the
existing API contract rather than a change to architecture, data model, security model, or
technology choice.
