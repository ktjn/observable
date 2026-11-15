# Feature proposal: push-based updates for read-only views

**Status:** Draft — requirements and tests require requester approval before implementation.
**Baseline:** `main` after auth tenant-selection change (PRs #945 and #946).
**Owner:** Observable platform, `observable-query` + `observable-web` and domain event providers.
**Scope:** Browser operational read-only views. No changes to tenant selection or authentication APIs.

## 1. Goal and boundaries

Replace fixed-interval polling with server-managed view subscriptions. SSE carries authorized initial snapshots and incremental or replacement **data**, not only invalidation. Existing query capabilities remain authoritative for access, query semantics and data; the subscription manager reuses them. Invalidation-only hints remain an explicit fallback for query types where streaming results is impractical.

**MVP:** service health, active alert state, and deployment/change-marker status. **Subsequent slice:** dashboard metric panels, log/trace search refresh, and optional bounded log tail. Pinned historical views do not subscribe. Mutable forms and editing sessions are out of scope. This is not a blanket prohibition on local time-range clocks.

Changes to `X-Tenant-ID`, JWT contents, `POST /v1/auth/switch-tenant`, and per-tenant URLs are explicitly **out of scope**. The header has already been removed. Browser reads use the active authenticated session with an `HttpOnly` cookie, and API keys remain tenant-bound.

## 2. Current baseline verified on main

- `services/query-api/src/middleware/auth.rs`: bearer API keys are resolved through `/internal/validate`; browser session cookies and bearer session JWTs use `/internal/validate-session`; no tenant header.
- `services/auth-service/src/oidc.rs`: `POST /v1/auth/switch-tenant` authorizes membership, updates the stored session tenant and issues a new `session` cookie; `resolve_session` uses the database session tenant rather than a client tenant selector.
- `apps/frontend/src/api/logs.ts`: network tenant headers are empty (the old `tenantId` function argument is transitional).
- `apps/frontend/src/hooks/useLiveRefresh.ts`: recurring 5-second query polling remains; `useGlobalDateRange.ts` advances rolling time locally; `useLiveTail.ts` and `LogLiveTail.tsx` poll.
- `spec/adr/ADR-035-component-independence.md`: metadata ownership stays with the provider; query can consume released domain event contracts, never another component's PostgreSQL state.

## 3. Transport and server-side client context

Native same-origin `EventSource` uses the existing `HttpOnly` session cookie. The server retains **one context per browser tab**, multiplexing its active read-only views. Context is never an authorization authority: current tenant, identity, role and permitted resources are resolved from the stored session and checked on query execution and each data delivery.

### 3.1 Registration lifecycle

```http
POST /v1/ui/subscriptions
Cookie: session=<jwt>
Content-Type: application/json

{"views":[{"viewId":"health","query":{"kind":"service-health","service":"payments"},"timeRange":{"mode":"rolling","window":"15m"}}]}
```

Returns `{"subscriptionId":"opaque-id","revision":"r1","expiresAt":"..."}`.

```http
GET /v1/ui/subscriptions/{subscriptionId}/events
Cookie: session=<jwt>
Accept: text/event-stream
```

`PUT /v1/ui/subscriptions/{subscriptionId}` atomically replaces the view definitions when filters or routes change; `DELETE /v1/ui/subscriptions/{subscriptionId}` provides best-effort cleanup. Closed/disconnected/expired sessions and view leases are reclaimed server-side even without DELETE. Only bounded, allowlisted typed queries are accepted. POST/PUT/DELETE enforce same-origin CSRF protection. Subscription IDs are unguessable opaque handles, **not credentials**; every access checks the cookie and session binding.

### 3.2 Tracked context

```typescript
interface ViewSubscription {
  id: string;
  sessionId: string;
  tenantId: string;
  principalId: string;
  authorizationRevision: string;
  views: Array<{
    viewId: string;
    query: QueryDefinition;
    filters: Record<string, unknown>;
    timeRange: TimeRange;
    queryRevision: string;
    lastDeliveredRevision?: string;
  }>;
  lastEventCursor?: string;
  leaseExpiresAt: string;
}
```

The server uses this context to track active client views, share eligible query work, select relevant events, enforce quotas, observe the number of subscribers, and expire resources. Distinct tabs remain separate subscriptions. Context must work across multiple query-api replicas without sticky-session assumptions; loss of registration state forces re-registration and an authoritative snapshot.

### 3.3 SSE data protocol

```text
id: opaque-101
event: snapshot
data: {"v":1,"viewId":"health","queryRevision":"r1","revision":"1842","data":{"service":"payments","health":"normal"}}

id: opaque-102
event: delta
data: {"v":1,"viewId":"health","queryRevision":"r1","revision":"1843","changes":[{"op":"upsert","key":"payments","value":{"health":"degraded"}}]}

event: replace
data: {"v":1,"viewId":"health","queryRevision":"r1","revision":"1844","data":{"service":"payments","health":"normal"}}

event: invalidate
data: {"v":1,"viewId":"expensive-aggregate","queryRevision":"r1"}

event: reset
data: {"v":1,"reason":"cursor_unavailable"}
```

- **snapshot:** full authorized initial view result, with registration/event observation established first to avoid the snapshot/subscribe race.
- **delta:** typed, ordered, revisioned upsert/delete operations where the view model supports deterministic incremental application.
- **replace:** complete fresh result for smaller aggregate or state views; not a separate query engine.
- **invalidate:** fallback hint requesting normal query API refetch for unsupported or expensive delta projections.
- **reset:** cursor gap, expired context, changed permissions, buffer overflow, restart without replay state, or incompatible schema; client re-registers or refetches an authorized snapshot.
- Events are delivered at least once. Every data event is schema- and size-bounded and tagged by view, query revision and result revision. Client rejects duplicates, stale revisions and results for replaced view definitions. `Last-Event-ID` resumes only when session, tenant, scope, query revision and replay history all match.
- SSE comments provide heartbeats. Enforce `Cache-Control: no-store`, streaming proxy configuration and no buffering. Never put JWTs, raw session secrets or client tenant selectors in the event URL.

### 3.4 Correctness, authorization and recovery

1. On registration, authorize each query using the existing query facade, then establish change observation and produce the initial snapshot or reconcile any changes during snapshot execution.
2. Domain publishers emit versioned change events only after their state is query-visible. The subscription manager selects matching registered views and performs an authorized query, calculates a safe typed delta, or emits an authoritative replacement.
3. The client applies data events directly to its typed view state or TanStack Query cache. There is **no mandatory second GET** after a data event; invalidation falls back to GET only when required.
4. For rolling ranges, expire old buckets independently of new events; fixed historical views do not subscribe.
5. Session tenant switching, logout, revocation and membership/role changes terminate old subscriptions. Server revalidates authoritative session and resource permission **before emitting any sensitive data batch**, even when switching occurs in a different tab. Identity-service outage fails closed.
6. Enforce quotas on per-session tabs/views, registration updates, memory, pending results, emitted bytes, query execution and reconnects. Coalesce provider events. On overflow, send reset rather than silently dropping data.
7. On SSE outage or background-tab return, reconcile by snapshot or bounded fallback polling and show live/reconnecting/stale/paused/unauthorized state.
8. Server tracks active subscription count, view count, stream lag, delivered payload bytes, TTL expiry, backpressure, reset count and authorization failures through safe low-cardinality observability labels.

## 4. Component handoff obligations

`PUSH-001` is the parent platform requirement. Every provider-specific subrequirement must refine `PUSH-001` in `spec/system.yaml`; parent integration verification is separate.

| ID | Owner | Obligation | Priority |
| --- | --- | --- | --- |
| PUSH-001 | platform integration | Server-managed read-only views receive authorized snapshots and incremental/replacement data without fixed query polling | P1 |
| PUSH-002 | observable-query | Provide authenticated registration, server-owned client context and resumable data-bearing SSE | P1 |
| PUSH-003 | observable-control | Publish query-visible deployment/change-marker invalidation events through versioned broker contract | P1 |
| PUSH-004 | observable-web | Register and update view context; apply SSE snapshots, deltas and replacements to view cache | P1 |
| PUSH-005 | observable-web | Preserve filters, time range, selection and read-only state during updates | P1 |
| PUSH-006 | observable-query | Signal replay gaps/overflow and enforce resync or stop; never imply silently complete delivery | P1 |
| PUSH-007 | observable-query | Bind every stream and event batch to the authenticated active session tenant and resource permissions | P1 |
| PUSH-008 | observable-query | Bound connection count, memory, fan-out, event rate, query amplification and reconnect storms | P1 |
| PUSH-009 | observable-web | Expose live/reconnecting/stale/paused/unauthorized UI states and controlled fallback | P1 |
| PUSH-010 | observable-web playground | Use programmable event-source fake in browser-only playground, including failures and tenant switch | P1 |
| PUSH-011 | platform integration | Independently verify snapshot/delta/replace event-to-render, authorization and convergence | P1 |
| PUSH-012 | observable-health | Emit state-change notifications only after health assessment is query-visible | P1 |
| PUSH-013 | observable-alerting | Emit alert/incident state-change notifications after commit and authorization filtering | P1 |
| PUSH-014 | observable-web | Stop old and establish new streams on tenant switch, logout and multi-tab session changes | P1 |
| PUSH-015 | observable-web | Retain local rolling-range progression and suppress push refresh for pinned historical data | P1 |
| PUSH-017 | observable-query | Track server-owned per-tab/per-view context, query revisions, lease TTL and lifecycle metrics | P1 |
| PUSH-018 | observable-query | Deliver authorized snapshot/delta/replace data; invalidate only when appropriate | P1 |
| PUSH-019 | observable-web | Apply SSE result revisions idempotently and reject obsolete view-query revisions | P1 |
| PUSH-016 | observable-store-clickhouse + query (future slice) | Publish/coalesce query-visible telemetry watermarks for metric/log/trace streaming | P2 |

No new auth capability is required. `observable-auth` remains the identity/authorization dependency; do not add cross-owner database queries, JWT claims, a new tenant header, or a second tenant-switch API.

### Mock contracts and independence

- Provider-owned programmable mocks (control, health, alerting) must generate committed, duplicate, delayed, unauthorized and unavailable event sequences. The query provider publishes a programmable subscription-registry and SSE fake with register/update/delete, snapshot/delta/replace/invalidate/reset, open/close/retry/gap/overflow and auth-switch scenarios.
- `observable-web` tests depend on the public SSE facade and the query runtime contract, never the broker or Rust implementation.
- Verify each provider's real contract and fake against the same versioned event schema. The platform join separately proves `PUSH-001`.

## 5. UX contract

- On the first live view render, show the initial scoped snapshot; once the stream is connected and reconciled, show a subtle `Live` indicator. Do not flash, jump scroll, reorder a selected row or discard comparison overlays.
- On reconnect or fallback, show last successful update time and a stale/reconnecting indicator. Preserve cached read-only data where still authorized; never display stale data as current health.
- On a tenant switch, suppress rendering of the previous tenant's snapshot immediately. Reconnect after `/v1/auth/switch-tenant` completes and `/v1/auth/me` confirms the selected tenant.
- For pinned/historical range, indicate `Paused`/historical rather than `Live`; event delivery must not change the snapshot.
- Use an accessible status announcement for transitions, not each incoming event. Respect reduced-motion preferences.

## 6. Non-functional acceptance (initial proposed budgets)

| Metric | Proposed target |
| --- | --- |
| Committed control/health/alert state → updated view | p95 ≤ 2 seconds, excluding a query-backend outage |
| Healthy idle live view | Zero periodic **network** query polling; local rolling-window clock still permitted |
| Browser event connections | At most one active SSE per tab/session origin |
| Server query amplification | Bounded coalesced query execution; data events do not require a second browser GET |
| Backpressure | Bounded buffers; overflow triggers explicit reset and full refresh |
| Tenant isolation | Zero cross-tenant events or cached data disclosure under switch/revocation |
| Stream failure recovery | Eventual convergence via replay, reset or bounded fallback; no permanent silently stale state |

These are review targets, not measured outcomes. Determine the coalescing window, heartbeat interval, fallback cadence, replay retention and load envelope through representative benchmarks before normative approval.

## 7. Acceptance-test matrix (proposed)

| Test ID | Requirement | Scenario / pass condition |
| --- | --- | --- |
| PUSH-T01 | 001,004,011 | An authorized service-health subscription receives snapshot and delta data and renders them without another GET |
| PUSH-T02 | 003,011 | Deployment/change-marker event is emitted only after the authoritative read sees the new revision |
| PUSH-T03 | 004,005 | Unrelated service event does not update unrelated subscriptions; selection and filters persist |
| PUSH-T04 | 002,006 | Duplicate and out-of-order events never regress a displayed revision |
| PUSH-T05 | 002,006,009 | Disconnected stream reconnects; lost cursor produces reset and authoritative resynchronization |
| PUSH-T06 | 007,014 | Tenant A → B switch closes A stream, clears A results and starts an authorized B stream |
| PUSH-T07 | 007,014 | Tenant switch from a second tab terminates A's server-side stream without relying on tab messages |
| PUSH-T08 | 007 | Revoke membership/session while connected; no further restricted data or existence metadata reaches client |
| PUSH-T09 | 002,007 | Missing/invalid session, disallowed resource scope and auth-service outage fail closed |
| PUSH-T10 | 008 | Bursty events and slow consumer keep memory/query fan-out bounded; reset on overflow |
| PUSH-T11 | 015 | Pinned historical views do not refetch; rolling range advances and expires old buckets correctly |
| PUSH-T12 | 009 | SSE blocked → bounded polling fallback + visible degraded mode; service restoration converges |
| PUSH-T13 | 010 | WASM playground reuses the stream contract and simulates success, delay, gap, tenant switch and failure |
| PUSH-T14 | 012,013 | Health and alert providers satisfy real/mock event publishing contracts after commit |
| PUSH-T15 | 011 | Multi-instance fan-out works behind Compose/Kubernetes gateway without sticky-session assumptions |
| PUSH-T16 | 011 | Actual browser E2E flow verifies server query context → SSE snapshot/delta/replace → visible UI changes |

| PUSH-T17 | 017 | Register/update/delete maintains distinct tab contexts, lease expiry and replica-independent recovery |
| PUSH-T18 | 018,019 | Duplicate, out-of-order, stale query revision and oversized data events are rejected or reset without corrupting cache |

Tests must be traceable to **one** subrequirement each in the machine-checkable spec; this scenario matrix is the review index, not the final `verifies` annotation map. Add positive, negative, boundary, failure and restart cases in owned test suites before implementation.

### Existing authorization prerequisite

Current `auth-service/src/oidc.rs::resolve_session` reads `user_tenant_roles` with a left join and can fall back to the JWT's previous `role` when the active membership row is absent. An SSE connection must not treat that fallback as current authorization. Verify or correct this fail-closed behavior in an independently reviewed authentication/security slice **before** enabling live push. This does not authorize new tenant-selection mechanisms or change the scope of the header-removal task.

## 8. Risks

| Risk | Mitigation and evidence |
| --- | --- |
| Stream survives tenant switch or revocation | Server-side session tenant revalidation before emission, independent of client tab cleanup; negative browser and backend tests |
| Missed events or delayed broker write | Durable cursor where available, reset on uncertainty, fall back to authoritative reads; reconnect and storage outage tests |
| Push-before-query-visibility race | Commit/visible-watermark publication; query replay tests |
| Query storm from telemetry updates | Topic coalescing, targeted invalidation, rate limits, histogram/window bucketing; workload measurements |
| Client drift during rolling window | Separate local clock advancement from network invalidation; fixed-range tests |
| Proxy-buffered or silently dropped SSE | Same-origin distribution smoke, no-store and heartbeat checks, explicit stale-status detection |
| Business/tenant event metadata leakage | Sanitized hints and per-resource access checks; existence-hiding tests |

## 9. Implementation waves — only after approval

1. **Contract review:** approve requirements, subrequirements, failure modes, acceptance cases, API schema, ADR, and component impact with `system-spec --impact` / `--compat`.
2. **Provider-first:** independently implement control, health and alerting event publication with programmable mocks and real-provider contract suites.
3. **Subscription facade:** implement `observable-query` subscription registry, registration lifecycle, authorized snapshot/delta/replace delivery, cursor, auth revalidation, pressure handling and distributed fan-out, using provider fakes.
4. **Frontend:** implement a single stream manager, view registration, revision-aware SSE result application, fallback invalidation, status UI and tenant-switch lifecycle; add programmable playground adapter.
5. **Join & rollout:** browser/full-stack tests, load and resilience exercises, compare against 5-second polling, progressively disable old polling per view, and record requirement-level evidence.
6. **Later scope:** telemetry visibility-watermark updates for metric panels and logs/traces. Separate acceptance gate; not assumed delivered by MVP.

## 10. Validation and approval gate

- `spec/system.yaml` adds only **draft P1/P2** obligations; current P0 release evidence is untouched.
- Run `system-spec spec/system.yaml --impact <id>` on changed capabilities, `--compat` against the previous main spec, `bash scripts/spec-gate.sh`, and scoped mock/contract checks.
- Owner reviews requirements and tests **before** any implementation PR. No new requirement is labeled verified without current criterion-specific evidence.
- Reassess existing ADR-031 (tenant context) only for integration assumptions; this feature introduces **no auth redesign**.
