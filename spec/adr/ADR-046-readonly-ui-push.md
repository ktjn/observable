# ADR-046: Server-managed view subscriptions with data-bearing SSE

**Date:** 2026-11-15
**Status:** Proposed — requires review
**Deciders:** Observable project stakeholders

## Context

Observable's read-only UI uses fixed-interval polling (5-second shared live-view refresh, plus specialized log-tail polling). After PRs #945 and #946, `X-Tenant-ID` is removed. Browser requests authenticate with the `HttpOnly` session cookie; the active tenant is server-authoritative and is switched via `POST /v1/auth/switch-tenant`, which updates session state and reissues the cookie.

Read-only views should receive timely updates without repeatedly querying unchanged data. Existing query APIs and domain owners remain authoritative, per ADR-035.

## Proposed decision

**Design review (2026-11-15) simplified this ADR from its first draft** (grilled against the recorded design-tree session): single-call register-and-stream, ephemeral in-memory context, a minimal fixed-size-queue backpressure floor, and no ACK endpoint for MVP. The adaptive flow-control/lag-skip/ACK subsystem is deferred to a later slice (`PUSH-021`–`PUSH-023`, P2).

1. Expose native same-origin SSE at **`GET /v1/ui/subscriptions/events`**. View definitions are carried on this same request (e.g. a `views` query parameter) — there is no separate `POST`/`PUT`/`DELETE` lifecycle or `subscriptionId`. Session cookie authentication is mandatory, with no tenant header. To change view definitions, the client closes the connection and reopens it.
2. Track **client view context in memory on the server, for the connection's lifetime only**: session, authoritative tenant, principal, authorized normalized queries, filters, time ranges and revisions. Nothing durable is written. Disconnect, reload or losing the backing replica simply ends the context; the client reconnects and re-sends its view definitions, receiving a fresh authorized snapshot. This needs no distributed lease store and no sticky-session routing, because nothing survives past the connection.
3. SSE carries **initial authorized snapshots, typed revisioned deltas, and full replacement data**. `delta` is restricted to a pass-through of an already-known upsert/delete op taken directly from the domain event — the server never computes a diff; anything else uses `replace`. The browser applies events directly to its view state or TanStack Query cache. Optional `invalidate` hints fall back to normal query refetch for unsupported query shapes; a `reset` forces an authoritative snapshot.
4. Reuse existing provider query capabilities for result execution and access decisions; **do not create a second query engine**.
5. Providers publish changes only once query-visible: `observable-control` for deployments/change markers, `observable-health` for health, and `observable-alerting` for alert state. Use released event contracts and broker fan-out; no cross-owner SQL or implementation imports.
6. Bind a stream to the session's authoritative active tenant at establishment. On session tenant change/revocation, server revalidation prevents disclosure regardless of client cooperation. On the frontend, stop, clear/partition cache, and reconnect after successful tenant switch. Reconcile other tabs.
7. Use at-least-once delivery semantics. A small in-memory per-connection replay buffer backs `Last-Event-ID` resume; anything the buffer can't cover (evicted, restart, oversized gap) sends explicit `reset` and requires an authoritative re-fetch. Use bounded polling fallback when SSE is unavailable.
8. Preserve local progression of rolling time ranges and do not refetch pinned historical snapshots.
9. **MVP backpressure floor:** a fixed-size per-connection queue. On overflow, disconnect the connection; the client reconnects and receives a fresh snapshot. No conflation, no fair per-view scheduling, no partial `supersedesThrough` replacement in MVP — see the deferred slice below.
10. **Deferred to a later slice (P2, not MVP):** adaptive per-view fair scheduling with configurable queue bytes/age/write-timeout thresholds; soft-lag conflation and hard-lag skip-to-latest via a complete `replace` carrying an explicit superseded revision boundary; and an optional rate-limited per-view `POST /v1/ui/subscriptions/{id}/ack` reporting last *applied* revision as advisory browser-processing-lag feedback (never authorization, never required for correctness). Re-promote `PUSH-021`–`PUSH-023` to P1 only when this slice is scheduled.

No `X-Tenant-ID`, tenant ID in stream URL, new JWT claims, or separate tenant-switch API is introduced. API-key SSE streaming is excluded from the first browser-only delivery.

## Consequences

- One-way delivery is simpler than WebSockets and removes the need for fetch streaming to add custom tenant headers.
- Existing data query contracts and permissions remain authoritative; server-side context is in-memory and scoped to the connection, so it incurs no durable registration or lease cost. Data delivery avoids mandatory client refetch.
- Long-lived HTTP connections require gateway/proxy buffering and timeout configuration, fan-out admission control, and revalidation after tenant switching or revocation. Lag/coalescing policy is deferred past MVP; MVP's only safety net is disconnect-and-reconnect on queue overflow.
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
- **Security prerequisite (separate gating ticket, not a footnote):** current `auth-service/src/oidc.rs::resolve_session` falls back to JWT role when the active membership is missing. This cannot establish current membership for push disclosure. Track and land an independently reviewed fail-closed authorization fix as its own PR before the subscription-facade implementation wave starts — do not fold it into a push-feature PR.
- Every new event publisher exposes a programmable fake and provider contract tests as required by `system-spec/FEATURE.md`.

## References

- [Feature plan](../../docs/plans/push-based-readonly-views.md)
- [ADR-031](ADR-031-global-tenant-environment-context.md)
- [ADR-035](ADR-035-component-independence.md)
- [System Specification](../system.yaml)
