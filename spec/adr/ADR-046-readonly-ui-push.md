# ADR-046: Server-managed view subscriptions with data-bearing SSE

**Date:** 2026-11-15
**Status:** Proposed — requires review
**Deciders:** Observable project stakeholders

## Context

Observable's read-only UI uses fixed-interval polling (5-second shared live-view refresh, plus specialized log-tail polling). After PRs #945 and #946, `X-Tenant-ID` is removed. Browser requests authenticate with the `HttpOnly` session cookie; the active tenant is server-authoritative and is switched via `POST /v1/auth/switch-tenant`, which updates session state and reissues the cookie.

Read-only views should receive timely updates without repeatedly querying unchanged data. Existing query APIs and domain owners remain authoritative, per ADR-035.

## Proposed decision

1. Expose a **server-managed subscription registry** through `POST/PUT/DELETE /v1/ui/subscriptions[/{subscriptionId}]` and native same-origin SSE at `GET /v1/ui/subscriptions/{subscriptionId}/events`. Session cookie authentication is mandatory, with no tenant header.
2. Store per-tab **client view context** server-side: session, authoritative tenant, principal, authorized normalized queries, filters, time ranges, revisions, lease and event cursor. The registered context must work across replicas without sticky-session assumptions.
3. SSE carries **initial authorized snapshots, typed revisioned deltas, and full replacement data**. The browser applies them directly to its view state or TanStack Query cache. Optional `invalidate` hints fall back to normal query refetch for unsupported query shapes; a `reset` forces an authoritative snapshot.
4. Reuse existing provider query capabilities for result execution and access decisions; **do not create a second query engine**.
5. Providers publish changes only once query-visible: `observable-control` for deployments/change markers, `observable-health` for health, and `observable-alerting` for alert state. Use released event contracts and broker fan-out; no cross-owner SQL or implementation imports.
6. Bind a stream to the session's authoritative active tenant at establishment. On session tenant change/revocation, server revalidation prevents disclosure regardless of client cooperation. On the frontend, stop, clear/partition cache, and reconnect after successful tenant switch. Reconcile other tabs.
7. Use at-least-once delivery semantics, opaque resume cursor, explicit `reset` on gaps/overflow, scoped queries for resync, and bounded polling fallback when SSE is unavailable.
8. Preserve local progression of rolling time ranges and do not refetch pinned historical snapshots.
9. Implement **bounded per-view and per-connection flow control**, with fair scheduling and configurable queue bytes, event count, oldest-age and write-timeout thresholds. On soft lag conflate superseded state; on hard lag automatically **skip intermediate deltas** only for replaceable state views, sending a complete authorized latest-state `replace` with explicit superseded revision boundary. When a full replacement is unavailable, send `reset` and resynchronize.
10. Differentiate transport pressure from browser processing lag: optional rate-limited per-view `POST /v1/ui/subscriptions/{id}/ack` reports last *applied* revision. ACKs are advisory, not required for correctness or used as authorization; validate session and monotonic revision. Never silently skip non-replaceable audit/history streams.

No `X-Tenant-ID`, tenant ID in stream URL, new JWT claims, or separate tenant-switch API is introduced. API-key SSE streaming is excluded from the first browser-only delivery.

## Consequences

- One-way delivery is simpler than WebSockets and removes the need for fetch streaming to add custom tenant headers.
- Existing data query contracts and permissions remain authoritative; server-side context incurs lease, registration and query execution costs. Data delivery avoids mandatory client refetch.
- Long-lived HTTP connections require gateway/proxy buffering and timeout configuration, fan-out admission control, bounded queues, lag/coalescing policy, and revalidation after tenant switching or revocation.
- A stale cookie alone must never authorize emission after stored-session tenant or permissions change. Cross-tab notifications are an optimization, not the security boundary.

## Alternatives

- **WebSockets:** rejected for one-way read-only notifications; additional connection protocol and heartbeats offer no advantage for MVP.
- **Invalidation-only SSE:** retained as fallback, not primary: mandates an extra query round trip and cannot reuse server-side view context to deliver a materialized result.
- **Keep fixed polling as primary:** rejected for healthy sessions due to idle query cost; retained only as fallback.
- **Add tenant to URL or request header:** rejected; active tenant is already authoritative in authenticated session.
- **Direct browser-to-Redpanda subscription:** rejected; violates domain and authorization boundaries.

## Security and implementation prerequisites

- Authorization checks must fail closed when session/permission state cannot be established.
- A client must not see the existence or identifying metadata of restricted resources through event types, counts, or replay cursors.
- Streaming auth-state changes and the old connection's lifetime must be tested separately from single-tab switch behavior.
- **Security prerequisite:** current `auth-service/src/oidc.rs::resolve_session` falls back to JWT role when the active membership is missing. This cannot establish current membership for push disclosure. Require a separate verified fail-closed authorization fix before implementation is enabled.
- Every new event publisher exposes a programmable fake and provider contract tests as required by `system-spec/FEATURE.md`.

## References

- [Feature plan](../../docs/plans/push-based-readonly-views.md)
- [ADR-031](ADR-031-global-tenant-environment-context.md)
- [ADR-035](ADR-035-component-independence.md)
- [System Specification](../system.yaml)
