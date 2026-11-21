# ADR-047: WASM-Native Local Observability Engine

**Date:** 2026-11-21
**Status:** Accepted (Phases 1–3 implemented; Phase 4 partial — see `docs/plans/wasm-local-observability-engine.md`)
**Authors:** Observable platform (grilled design session, Claude Sonnet 5)
**Deciders:** Observable project stakeholders
**Review date:** 2027-02-21

## Context

The frontend's "playground" mode (`apps/frontend/src/playground/`) runs a DuckDB-WASM engine so the app is demoable without a backend. Per spec §21.3/21.6 (`spec/21-frontend-modules.md`), "Production HTTP and playground/local implementations MUST implement the same capability contract" and the playground "MUST be runnable without the production backend."

Today that parity is capability-level, not implementation-level: `httpRuntime.ts` and `playgroundRuntime.ts` are two independently maintained `RuntimeApi` builds. A gap-finding pass found concrete divergence beyond that split:

- Query logic is partially shared (bucket/rate math lives in a Rust crate compiled to WASM, reused verbatim from `discovery.rs`), but hand-written convenience paths in `engineWorker.ts` (log search/tail/context) bypass it — no facets, no total count, no tenant bind, vs. the real `search_logs`.
- The push wire protocol is faithfully shared (`playgroundViewSubscriptions.ts` implements the real ADR-046 snapshot/delta/replace/revision model), but `liveTick.ts` is a second, disconnected mechanism that mutates storage directly on its own timer instead of emitting through that protocol.
- Storage is a flat, unscoped, ephemeral set of DuckDB tables — no tiering (ADR-012), no durability.
- The `logs` table has no `tenant_id` column at all; a single hardcoded demo tenant stands in for ADR-007's per-query tenant binding.
- Alert/health fields are hardcoded (`slo_breaching: false, active_alert_count: 0`) rather than evaluated; the real enrichment path (alert-evaluator/control-service) is never called.

Separately, the goal for this engine has grown past "demo fallback": the intent is a genuinely usable, fully client-side observability backend that can be embedded directly on sites the operator controls, with no server, for real local monitoring — not just a sales/onboarding sandbox.

## Decision

1. **Transport collapse.** Replace the two independently-built `RuntimeApi` implementations with one runtime factory parameterized by a transport. Introduce a shared route table (method + path, matching the real REST paths in `spec/09-api.md`) that `api/*.ts` dispatches through instead of calling `fetch()` directly. Exactly two transports implement it: `httpTransport` (real `fetch`) and `wasmTransport` (in-process dispatch into the engine). Mode is **strict and boot-time**: one site instance is entirely hosted or entirely local, chosen once, never mixed per route.
2. **SSE stays its own seam.** `ViewSubscriptionTransport` (`viewSubscriptionProtocol.ts`) is not folded into the route-table dispatcher above — its lifecycle (long-lived stream, revisions, resets) is already faithfully modeled and forcing it through a request/response shape would require working around its own semantics. `liveTick.ts` is rewired to emit through this existing `ViewSubscriptionEvent` pipeline instead of mutating storage directly, so simulated live data flows through the same event model the hosted product uses.
3. **Durable local storage.** The local engine's store moves from ephemeral in-memory DuckDB tables to OPFS/IndexedDB-backed persistence, so monitoring state survives a browser restart.
4. **Shared evaluation logic.** Alert and health evaluation extend the existing shared-Rust-to-WASM pattern already proven by the query crate: the real `alert-evaluator`/`observable-health` logic compiles to WASM too, instead of the engine hardcoding "nothing is breaching."
5. **Tenant threading.** Every table and query in the local engine carries and binds `tenant_id` consistently with the real query layer (ADR-007), even though a local instance has exactly one implicit tenant — so the same query code runs unmodified in both modes.
6. **Self-instrumentation.** The engine emits its own logs/traces/metrics through the same pipeline it exposes to callers, into a local equivalent of the `system`/`platform` tenant, mirroring the recursive in-band model in `spec/17-self-observability.md` §2.1.
7. **Delivery: no browser extension.** The engine ships as an embeddable module that a site the operator controls includes directly — self-contained per origin, no host-permission model, no content scripts. Any client-side instrumentation of that site reuses the real OpenTelemetry Browser SDK packages (`@opentelemetry/sdk-trace-web` + fetch/XHR/document-load instrumentations + `web-vitals`), not a hand-rolled equivalent, consistent with ADR-001 and ADR-018's (never-implemented) browser SDK design.
8. **Cross-site aggregation is deferred.** Per-site isolation (each embedded instance has its own local store, no shared view across origins) ships first. If a unified view across an operator's own sites is later needed, the preferred mechanism is WebRTC for the data path with manual/local (no-server) signaling between same-machine peers — not a signaling server, and not a browser extension providing a shared context.

## Consequences

**Easier:**
- One request-handling code path instead of two; a route added to the real API is added once, not twice.
- The local engine becomes a legitimate standalone capability, not just a fallback for when the backend is down.
- Self-instrumentation gives the engine's own correctness a verification signal independent of the thing it's monitoring.

**Harder:**
- The local engine now owns real durability (OPFS/IndexedDB) and real evaluation logic (alerting/health), which is materially more implementation than "fake the shape of the response."
- Shared Rust-to-WASM evaluation logic must stay compilable to both targets as the server-side logic evolves.

**Constrained:**
- Strict boot-time mode means a site instance cannot have a real backend for some capabilities and local WASM for others; that would require revisiting this ADR.
- No browser extension means cross-tab/cross-origin aggregation has no built-in browser-level channel; WebRTC-based sync (if built) needs its own signaling design.

## Alternatives Considered

### Option A: Browser extension (content script + background/offscreen WASM host)
Would let one install observe arbitrary other tabs without each site embedding anything. Rejected: scope-creeps into monitoring sites the operator doesn't control (a different consent/security model), and adds Manifest V3 lifecycle complexity (service-worker eviction, `chrome.offscreen`, host permissions) for a capability (cross-origin aggregation) that isn't needed for v1.

### Option B: `fetch()`/Service-Worker interception as the "same routing" mechanism
Intercepting the global `fetch` (à la MSW) to route to the WASM engine would let `api/*.ts` keep calling `fetch()` unmodified. Rejected in favor of an explicit `dispatch()` call through a shared route table: avoids interacting with the forthcoming OTel fetch auto-instrumentation (ADR-048) and avoids monkey-patching a global the rest of the app depends on.

### Option C: Mixed-mode transport selection per route
Would let a site use a real backend for some capabilities and local WASM for others. Rejected: no concrete requirement calls for it, and it introduces a security question (which routes are safe to localize) that strict mode avoids entirely.

### Option D: Ephemeral storage (status quo)
Simplest, matches today's demo-only playground. Rejected: a real local monitoring tool must survive a browser restart.

## Related

- [ADR-046](ADR-046-readonly-ui-push.md) — the push protocol `liveTick.ts` is rewired onto
- [ADR-036](ADR-036-frontend-module-boundaries.md) / `spec/21-frontend-modules.md` §21.3 — the capability-contract-parity requirement this ADR makes implementation-level
- [ADR-012](ADR-012-retention-tiering.md) — storage tiering precedent (not adopted for local storage in this ADR's scope, durability only)
- [ADR-007](ADR-007-multi-tenant-isolation.md) — tenant-binding model threaded into the local engine
- [ADR-018](ADR-018-rum-browser-mobile-ingestion.md) — browser SDK design reused for embedded-site instrumentation
- `spec/17-self-observability.md` §2.1 — recursive in-band model the engine's self-instrumentation mirrors
- [ADR-048](ADR-048-frontend-browser-self-instrumentation.md) — independent, parallel track reusing the same OTel Browser SDK decision
