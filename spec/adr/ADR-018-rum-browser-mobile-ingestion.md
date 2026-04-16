# ADR-018: RUM / Browser and Mobile Ingestion

**Date:** 2026-04-16
**Status:** Accepted
**Authors:** Claude Code
**Deciders:** Project Stakeholders
**Review date:** 2026-10-16

## Context

`spec/01-overview.md`, `spec/02-architecture.md`, and `spec/06-agents.md` list browser and mobile SDK ingestion as first-class capabilities. Real User Monitoring (RUM) signals differ from server-side OTLP in several important ways:

- **Origin:** Emitted from untrusted browser or mobile environments, not from controlled server processes
- **Protocol constraints:** Browsers cannot use gRPC; HTTP/1.1 and HTTP/2 with CORS are the practical transports
- **Session context:** RUM introduces a `session_id` and `user_hash` that do not exist in server-side OTel
- **Authentication:** No workload identity is available; authentication must use short-lived tokens issued per-session
- **Data volume and cardinality:** Browser events can arrive at very high rates with highly variable attribute sets (user agents, viewport sizes, navigation paths)
- **PII exposure:** Browser signals may contain URL parameters, form interaction paths, or DOM content that requires PII scrubbing before storage

No ADR previously addressed these differences or how the browser/mobile ingestion path fits into the OTel-first architecture.

## Decision

The platform will provide a **dedicated browser beacon intake endpoint and a mobile SDK intake endpoint**, both of which:

1. Accept signals via **OTLP/HTTP** (JSON or binary protobuf) over HTTPS. gRPC is not used for browser or mobile ingestion.
2. Require **session-scoped short-lived tokens** issued by the platform's auth service when a user session is established. Tokens encode `tenant_id`, `project_id`, and an expiry. No long-lived API keys are exposed to browser or mobile runtimes.
3. Enrich all received signals with `observable.session_id` and `observable.user_hash` (from the session token) before they enter the durable queue.
4. Apply **server-side PII scrubbing** at the intake boundary (URL normalization, attribute filtering, user agent normalization).
5. Route signals into the same durable queue as server-side OTLP, using `observable.ingest_source = browser_sdk` or `observable.ingest_source = mobile_sdk` for provenance.

**Browser SDK responsibilities:**
- Instruments `XMLHttpRequest`, `fetch`, and navigation timing to produce spans
- Propagates W3C `traceparent` on outbound requests to enable session ↔ backend trace correlation
- Captures Web Vitals (LCP, CLS, INP, FCP, TTFB) as OTel metrics
- Reports JS errors and unhandled promise rejections as log records with severity `ERROR`
- Manages `session_id` lifecycle (new session on first load, expiry after 30 min inactivity)
- Batches events and flushes on `visibilitychange` (pagehide) to minimize data loss on tab close

**Mobile SDK responsibilities:**
- Instruments HTTP client libraries for the target platform (iOS URLSession, Android OkHttp)
- Propagates W3C `traceparent` on outbound requests
- Reports app lifecycle events, crashes, and ANRs as log records
- Reports app startup time and frame rate as OTel metrics
- Manages `session_id` with a 30-minute inactivity timeout

**Session entity:**
A `Session` entity is stored in the control plane relational store (not ClickHouse) and indexed by `session_id`. It provides a lookup for session-level aggregation (session duration, page count, error rate per session) and is used to join session ↔ backend trace in the correlation engine.

| Field | Type | Notes |
|---|---|---|
| session_id | UUID | |
| tenant_id | UUID | |
| project_id | UUID | |
| user_hash | string | one-way hash; not reversible |
| started_at | timestamp | |
| last_activity_at | timestamp | |
| ended_at | timestamp | null if session is still active |
| sdk_type | enum(browser, ios, android) | |
| sdk_version | string | |
| app_version | string | |
| entry_url | string | first URL of session (browser); first screen (mobile) |
| attributes | map[string]string | user agent, device model, OS version, viewport |

## Consequences

**Easier:**
- Unified telemetry pipeline for server and client signals; no separate storage or query engine for RUM data
- Session ↔ backend trace correlation is first-class via `session_id` and W3C `traceparent` propagation
- OTel-compatible SDKs mean RUM data uses the same semantic conventions as server-side spans and logs

**Harder:**
- Browser intake endpoint must handle CORS preflight and enforce strict CSP-compatible token validation
- PII scrubbing at the intake boundary adds latency and complexity; scrubbing rules must be configurable per project
- Short-lived token issuance and refresh must be invisible to the end user and not cause telemetry gaps on token expiry

**Constrained:**
- Mobile SDK crash reporting (out-of-process crash capture) requires platform-specific native extensions and cannot be implemented in a pure OTel SDK; a thin native wrapper is required
- gRPC transport is not supported for browser/mobile; OTLP/HTTP is the only protocol

## Alternatives Considered

### Option A: Require all RUM signals to go through an OTel Collector sidecar
Route browser/mobile events through a server-side OTel Collector that proxies to the main ingest gateway.

**Rejected** because it requires customers to operate a CORS-enabled proxy, adds a network hop, and does not solve the PII scrubbing or session token problems.

### Option B: Build a separate RUM-specific ingestion pipeline with a proprietary payload format
Use a vendor-specific beacon format (similar to DataDog RUM or New Relic Browser agent).

**Rejected** because it violates the OTel-first principle (ADR-001) and creates a second data model that cannot be queried with the unified query facade.

## Related

- `spec/01-overview.md` §1.1 (browser/mobile RUM ingestion capability)
- `spec/06-agents.md` §10.1 (Browser SDK and Mobile SDK agent components)
- `spec/14-domain-model.md` §7 (`session_id` and `user_hash` common dimensions)
- `ADR-001-otel-external-contract.md` — OTel/HTTP is the transport; session context is additive
- `ADR-007-multi-tenant-isolation.md` — session tokens encode tenant context
