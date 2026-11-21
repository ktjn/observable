# ADR-048: Observable Frontend Browser Self-Instrumentation

**Date:** 2026-11-21
**Status:** Proposed
**Authors:** Observable platform (grilled design session, Claude Sonnet 5)
**Deciders:** Observable project stakeholders
**Review date:** 2027-02-21

## Context

`apps/frontend` has no OpenTelemetry browser instrumentation today (confirmed: no `@opentelemetry/*` dependency, no instrumentation code). `spec/17-self-observability.md` §2.1 already requires every platform service to emit its own OTLP logs/traces/metrics as a "first-class citizen" of the platform (recursive in-band monitoring); the frontend is not currently held to this.

This is independent of [ADR-047](ADR-047-wasm-local-observability-engine.md)'s local-engine work: it touches hosted mode on its own, has no dependency on ADR-047's transport collapse, and reuses only the instrumentation-standard decision (real OTel Browser SDK packages) from that ADR.

A known hazard applies the moment browser auto-instrumentation is added: `@opentelemetry/instrumentation-fetch` and `instrumentation-xml-http-request` patch `window.fetch`/`XMLHttpRequest` globally. If the telemetry exporter itself sends its batch via `fetch()`, that call is auto-instrumented too, producing a new span that queues for export, which calls `fetch()` again — an unbounded loop. This is orthogonal to ADR-047's transport design: ADR-047's `wasmTransport` is an explicit `dispatch()` call, not a `fetch()` interception, specifically so it is never caught by this instrumentation; the loop risk is confined to the exporter's own send path, in either hosted or playground mode.

Separately, the app's primary live-data path is `EventSource`-based SSE (`viewSubscriptionManager.ts`, ADR-046), which stock fetch/XHR instrumentation packages do not cover at all.

## Decision

1. Add the real OpenTelemetry Browser SDK (`@opentelemetry/sdk-trace-web` + `instrumentation-fetch` + `instrumentation-xml-http-request` + `instrumentation-document-load` + `web-vitals`) to the application shell, active in both hosted and playground modes — not a hand-rolled equivalent.
2. Prevent the export feedback loop using OTel JS's own mechanisms: `ignoreUrls` on the fetch/XHR instrumentation configs excludes the exporter's destination from being traced, and the exporter's send call is wrapped in `suppressTracing(context.active())`.
3. Add custom instrumentation for `EventSource`/SSE lifecycle — connection open/reconnect/stale/reset as spans or span events, tied to the status transitions `viewSubscriptionManager.ts` already tracks — since stock packages do not instrument `EventSource`. Add custom span attributes (`tenant_id`, `viewId`) that generic fetch instrumentation has no way to populate.
4. Route the output by mode: hosted mode exports to the real ingest-gateway under the self-observability `system`/`platform` tenant (`spec/17-self-observability.md` §2.1); playground mode exports into the local engine from ADR-047, treated as just another ingested source — no new sink is invented for playground mode.
5. This ships independently of ADR-047's foundation phase; it has no sequencing dependency on the transport collapse landing first.

## Consequences

**Easier:**
- Frontend performance, errors, and user-facing latency become visible through the same pipeline as everything else the platform monitors, rather than opaque.
- Normal app traffic (calls to `/v1/...`) is correctly traced by stock fetch/XHR instrumentation with no special handling — only the exporter's own send path needs loop suppression.

**Harder:**
- `EventSource` instrumentation is hand-maintained (no stock package covers it) and must track `viewSubscriptionManager.ts`'s status model as that evolves.

**Constrained:**
- Any future telemetry exporter transport must also exclude itself via `ignoreUrls`/`suppressTracing`, or the loop reappears.

## Alternatives Considered

### Option A: Hand-rolled instrumentation instead of the OTel Browser SDK
Rejected: duplicates a maintained standard implementation and contradicts the project's OTel-first principle (ADR-001); ADR-018's browser SDK responsibilities already describe this SDK's actual behavior.

### Option B: Skip SSE instrumentation
Rejected: `EventSource` is the app's primary live-data path (ADR-046); leaving it uninstrumented would make self-observability blind to the mechanism most live views depend on.

### Option C: Bundle into ADR-047's foundation phase
Rejected: no real coupling — this touches hosted mode on its own and would only make that phase's PR larger without a dependency reason.

## Related

- [ADR-047](ADR-047-wasm-local-observability-engine.md) — independent, parallel track; shares the OTel Browser SDK decision and the playground-mode export destination
- [ADR-046](ADR-046-readonly-ui-push.md) — the SSE protocol being instrumented
- [ADR-001](ADR-001-otel-external-contract.md) — OTel-first principle
- [ADR-018](ADR-018-rum-browser-mobile-ingestion.md) — browser SDK responsibilities this reuses
- `spec/17-self-observability.md` §2.1 — recursive in-band model this extends to the frontend tier
