# Feature plan: Observable frontend browser self-instrumentation

**Status:** Proposed, not started.
**ADR:** [ADR-048](../../spec/adr/ADR-048-frontend-browser-self-instrumentation.md).
**Owner:** `observable-web` (frontend).
**Scope:** Add real OTel Browser SDK instrumentation to the app shell, in both hosted and playground modes, without causing an export feedback loop. Independent of the [WASM local engine plan](wasm-local-observability-engine.md) — no sequencing dependency either direction.

## 1. Baseline (verified on main)

- `apps/frontend/package.json`: no `@opentelemetry/*` dependency.
- No existing instrumentation code (`grep -rl opentelemetry src` returns nothing but an unrelated text mention in `OnboardingWizard.tsx`).
- `apps/frontend/src/runtime/viewSubscriptionManager.ts`: the app's primary live-data path is `EventSource`-based SSE, which `@opentelemetry/instrumentation-fetch`/`instrumentation-xml-http-request` do not cover.

## 2. Dependencies to add

- `@opentelemetry/sdk-trace-web`
- `@opentelemetry/instrumentation-fetch`
- `@opentelemetry/instrumentation-xml-http-request`
- `@opentelemetry/instrumentation-document-load`
- `web-vitals`

## 3. Loop-prevention (must land before any instrumentation is active)

- Configure `ignoreUrls` on the fetch and XHR instrumentations to exclude the exporter's own destination (the ingest-gateway OTLP endpoint in hosted mode; the local engine's self-instrumentation sink in playground mode, per ADR-047 §6).
- Wrap the exporter's send call in `suppressTracing(context.active())` as a second, independent guard.
- Exit criteria: a test asserting the exporter's own network/dispatch call produces zero additional spans.

## 4. Custom instrumentation

- `EventSource` lifecycle spans/span-events (open, reconnect, stale, reset) driven off `viewSubscriptionManager.ts`'s existing `ViewSubscriptionStatus` transitions — no stock package covers this.
- Custom span attributes (`tenant_id`, `viewId`) attached at the points `viewSubscriptionManager.ts` already has that context.

## 5. Output routing

- Hosted mode: export to the real ingest-gateway under the self-observability `system`/`platform` tenant (`spec/17-self-observability.md` §2.1).
- Playground mode: export into the local engine (ADR-047), as just another ingested source — no separate sink.

## 6. Exit criteria (overall)

- Normal app navigation and API calls produce traces visible through the existing query path, in both modes.
- The exporter's own traffic never recurses into new spans.
- SSE connect/reconnect/stale/reset is visible as span data, correlated with `tenant_id`/`viewId`.
