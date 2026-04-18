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
