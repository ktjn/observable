# ADR-046: Session-scoped SSE invalidation for read-only operational views

**Date:** 2026-11-15
**Status:** Proposed — requires review
**Deciders:** Observable project stakeholders

## Context

Observable's read-only UI uses fixed-interval polling (5-second shared live-view refresh, plus specialized log-tail polling). After PRs #945 and #946, `X-Tenant-ID` is removed. Browser requests authenticate with the `HttpOnly` session cookie; the active tenant is server-authoritative and is switched via `POST /v1/auth/switch-tenant`, which updates session state and reissues the cookie.

Read-only views should receive timely updates without repeatedly querying unchanged data. Existing query APIs and domain owners remain authoritative, per ADR-035.

## Proposed decision

1. Expose **`GET /v1/ui/events`** from `observable-query` using standard SSE and same-origin cookie authentication. Browser clients use native `EventSource`, not a custom `fetch` stream for header transport.
2. Publish **versioned, scoped invalidation hints**, not full telemetry results. Invalidate active TanStack Query keys and refetch through existing query APIs.
3. Providers publish changes only once query-visible: `observable-control` for deployments/change markers, `observable-health` for health, and `observable-alerting` for alert state. Use released event contracts and broker fan-out; no cross-owner SQL or implementation imports.
4. Bind a stream to the session's authoritative active tenant at establishment. On session tenant change/revocation, server revalidation prevents disclosure regardless of client cooperation. On the frontend, stop, clear/partition cache, and reconnect after successful tenant switch. Reconcile other tabs.
5. Use at-least-once delivery semantics, opaque resume cursor, explicit `reset` on gaps/overflow, scoped queries for resync, and bounded polling fallback when SSE is unavailable.
6. Preserve local progression of rolling time ranges and do not refetch pinned historical snapshots.

No `X-Tenant-ID`, tenant ID in stream URL, new JWT claims, or separate tenant-switch API is introduced. API-key SSE streaming is excluded from the first browser-only delivery.

## Consequences

- One-way delivery is simpler than WebSockets and removes the need for fetch streaming to add custom tenant headers.
- Existing data query contracts and permissions remain the source of truth; stream notifications can be duplicated/dropped with deterministic reconciliation.
- Long-lived HTTP connections require gateway/proxy buffering and timeout configuration, fan-out admission control, backpressure, and revalidation after tenant switching or revocation.
- A stale cookie alone must never authorize emission after stored-session tenant or permissions change. Cross-tab notifications are an optimization, not the security boundary.

## Alternatives

- **WebSockets:** rejected for one-way read-only notifications; additional connection protocol and heartbeats offer no advantage for MVP.
- **SSE data payloads / full query deltas:** rejected; duplicates aggregation/query logic and creates a second authorization surface.
- **Keep fixed polling as primary:** rejected for healthy sessions due to idle query cost; retained only as fallback.
- **Add tenant to URL or request header:** rejected; active tenant is already authoritative in authenticated session.
- **Direct browser-to-Redpanda subscription:** rejected; violates domain and authorization boundaries.

## Security and implementation prerequisites

- Authorization checks must fail closed when session/permission state cannot be established.
- A client must not see the existence or identifying metadata of restricted resources through event types, counts, or replay cursors.
- Streaming auth-state changes and the old connection's lifetime must be tested separately from single-tab switch behavior.
- Every new event publisher exposes a programmable fake and provider contract tests as required by `system-spec/FEATURE.md`.

## References

- [Feature plan](../../docs/plans/push-based-readonly-views.md)
- [ADR-031](ADR-031-global-tenant-environment-context.md)
- [ADR-035](ADR-035-component-independence.md)
- [System Specification](../system.yaml)
