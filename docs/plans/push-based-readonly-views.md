# Feature proposal: push-based updates for read-only views

**Status:** Draft — requirements and tests require requester approval before implementation.
**Baseline:** `main` after auth tenant-selection change (PRs #945 and #946).
**Owner:** Observable platform, `observable-query` + `observable-web` and domain event providers.
**Scope:** Browser operational read-only views. No changes to tenant selection or authentication APIs.

## 1. Goal and boundaries

Replace fixed-interval query polling with authenticated server-driven change notifications for read-only operational views. Existing scoped query APIs remain authoritative for all view data; notifications indicate *which* cached queries may have become stale and do not carry telemetry results.

**MVP:** service health, active alert state, and deployment/change-marker status. **Subsequent slice:** dashboard metric panels, log/trace search refresh, and optional bounded log tail. Pinned historical views do not subscribe. Mutable forms and editing sessions are out of scope. This is not a blanket prohibition on local time-range clocks.

Changes to `X-Tenant-ID`, JWT contents, `POST /v1/auth/switch-tenant`, and per-tenant URLs are explicitly **out of scope**. The header has already been removed. Browser reads use the active authenticated session with an `HttpOnly` cookie, and API keys remain tenant-bound.

## 2. Current baseline verified on main

- `services/query-api/src/middleware/auth.rs`: bearer API keys are resolved through `/internal/validate`; browser session cookies and bearer session JWTs use `/internal/validate-session`; no tenant header.
- `services/auth-service/src/oidc.rs`: `POST /v1/auth/switch-tenant` authorizes membership, updates the stored session tenant and issues a new `session` cookie; `resolve_session` uses the database session tenant rather than a client tenant selector.
- `apps/frontend/src/api/logs.ts`: network tenant headers are empty (the old `tenantId` function argument is transitional).
- `apps/frontend/src/hooks/useLiveRefresh.ts`: recurring 5-second query polling remains; `useGlobalDateRange.ts` advances rolling time locally; `useLiveTail.ts` and `LogLiveTail.tsx` poll.
- `spec/adr/ADR-035-component-independence.md`: metadata ownership stays with the provider; query can consume released domain event contracts, never another component's PostgreSQL state.

## 3. Transport decision

A single **same-origin native `EventSource` connection per active browser tab** to:

```http
GET /v1/ui/events
Cookie: session=<jwt>
Accept: text/event-stream
```

No `X-Tenant-ID`, client tenant ID, access token in the URL, or bespoke `fetch` stream is required for this browser path. The standard EventSource connection automatically sends same-origin cookies. API-key/CLI streaming is not part of this feature.

The SSE endpoint belongs to `observable-query` as a browser-facing **notification facade**; the authoritative data continues to be fetched through existing query contracts. A future standalone fan-out service must preserve the identical released contract.

### 3.1 Stream protocol, v1

```text
: heartbeat

id: <opaque-event-cursor>
event: invalidate
data: {"v":1,"topic":"service.health","resourceType":"service","resourceId":"payments","revision":"1842","visibleAt":"2026-11-15T09:15:00Z"}

event: reset
data: {"v":1,"reason":"cursor_unavailable"}
```

- `invalidate` is a **hint**, not a state delta. `topic` is a versioned allowlisted value; initial values: `service.health`, `alert.state`, `deployment.state`, `change.marker`. Resource metadata is optional and emitted only if separately authorized. Unknown fields are ignored; unknown versions trigger a reset.
- The `id` is an opaque **tenant- and authorization-context-bound** replay cursor; never encode IDs or secrets in the cursor. The browser sends it as `Last-Event-ID` on reconnect when supported.
- `reset` requires a refetch of active subscribed queries after a bounded delay. Server restart, retention expiry, authorization change, replay gap, or queue overflow must result in `reset` rather than silently treating the view as current.
- Heartbeats are SSE comments and carry no tenant/resource data. Response headers include `Cache-Control: no-store`, `Content-Type: text/event-stream`, and proxy-buffering disabled through the supported distribution. No response-body caching or compression buffering.
- The server may deduplicate/coalesce events, but cannot promise exactly-once delivery. Downstream convergence, not event delivery count, is the correctness criterion.
- Native `EventSource` reconnects automatically. On authentication failure, frontend validates `/v1/auth/me` and moves to a stopped/unauthorized state; do not endlessly retry credentials known to be invalid.

### 3.2 Tenant and authorization lifecycle

- On initial connection, validate the current session through `observable-auth`; derive `tenant_id`, role, principal and authorized resources **server-side**. No tenant-selection query parameter, header, or event payload can override that context.
- The stream is bound to the session identifier and the **authoritative tenant active at establishment**. Before delivering any tenant-sensitive event batch, revalidate that the session still exists, is unrevoked, belongs to that tenant, and retains the necessary access. When identity infrastructure is unavailable, stop sending event data and fail closed.
- `POST /v1/auth/switch-tenant` changes the session tenant and cookie. The active tab closes its stream before switching, invalidates/partitions the old tenant cache, then reconnects after a successful switch. On failed switch, reconcile using `/v1/auth/me`; never display old tenant data under the new tenant.
- Other tabs may share the cookie and session; they receive cross-tab change notification where supported, and the **server independently terminates** stale-session-tenant streams even if client coordination fails.
- On logout, role/membership revocation or session expiry, stop the stream and clear inaccessible cached data. Denied resource events must not reveal existence, counts, names, or identifiers.
- Tenant, user, role, scope and cursor changes must not reuse another context's replay history. Enforce same-origin policy and explicit CORS restrictions; no wildcard credentialed SSE cross-origin access.

### 3.3 Query correctness

1. Subscribe first, then take the initial snapshot (or reconcile events received during the snapshot); avoid the subscribe/snapshot race.
2. After an authorized `invalidate`, map the event to specific **active** TanStack Query keys. Coalesce simultaneous invalidations before refetching, preserving existing data while refreshing.
3. Producers emit only after their state is committed and query-visible. For telemetry aggregates, use a storage visibility/watermark signal, never one browser event per raw span, log or metric.
4. Queries with fixed historical end times or pinned snapshots do not refetch merely because a new event arrives. Rolling windows still advance their time bounds locally and may need time-driven refresh to expire old buckets.
5. On replay gap, server restart, reconnect without provable continuity, or returning from a background tab, reconcile all affected live queries against the authoritative query API.
6. If SSE is blocked/unavailable, retain bounded, jittered polling as an operational fallback. Show `live`, `reconnecting`, `stale`, `paused`, `unauthorized` states; do not falsely label data fresh.

## 4. Component handoff obligations

`PUSH-001` is the parent platform requirement. Every provider-specific subrequirement must refine `PUSH-001` in `spec/system.yaml`; parent integration verification is separate.

| ID | Owner | Obligation | Priority |
| --- | --- | --- | --- |
| PUSH-001 | platform integration | Read-only live views converge to scoped authoritative results without fixed query polling in the healthy path | P1 |
| PUSH-002 | observable-query | Provide authenticated, resumable SSE change-hint stream with bounded fan-out | P1 |
| PUSH-003 | observable-control | Publish query-visible deployment/change-marker invalidation events through versioned broker contract | P1 |
| PUSH-004 | observable-web | Manage EventSource lifecycle and cache-key-specific query invalidation | P1 |
| PUSH-005 | observable-web | Preserve filters, time range, selection and read-only state during updates | P1 |
| PUSH-006 | observable-query | Signal replay gaps/overflow and enforce resync or stop; never imply silently complete delivery | P1 |
| PUSH-007 | observable-query | Bind every stream and event batch to the authenticated active session tenant and resource permissions | P1 |
| PUSH-008 | observable-query | Bound connection count, memory, fan-out, event rate, query amplification and reconnect storms | P1 |
| PUSH-009 | observable-web | Expose live/reconnecting/stale/paused/unauthorized UI states and controlled fallback | P1 |
| PUSH-010 | observable-web playground | Use programmable event-source fake in browser-only playground, including failures and tenant switch | P1 |
| PUSH-011 | platform integration | Independently verify end-to-end event-to-render, authorization and convergence | P1 |
| PUSH-012 | observable-health | Emit state-change notifications only after health assessment is query-visible | P1 |
| PUSH-013 | observable-alerting | Emit alert/incident state-change notifications after commit and authorization filtering | P1 |
| PUSH-014 | observable-web | Stop old and establish new streams on tenant switch, logout and multi-tab session changes | P1 |
| PUSH-015 | observable-web | Retain local rolling-range progression and suppress push refresh for pinned historical data | P1 |
| PUSH-016 | observable-store-clickhouse + query (future slice) | Publish/coalesce query-visible telemetry watermarks for metric/log/trace refresh | P2 |

No new auth capability is required. `observable-auth` remains the identity/authorization dependency; do not add cross-owner database queries, JWT claims, a new tenant header, or a second tenant-switch API.

### Mock contracts and independence

- Provider-owned programmable mocks (control, health, alerting) must generate committed, duplicate, delayed, unauthorized and unavailable event sequences. The query provider publishes a programmable SSE fake with open/close/retry/gap/overflow and auth-switch scenarios.
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
| Invalidation fetch amplification | At most one in-flight request per active query key per coalescing cycle |
| Backpressure | Bounded buffers; overflow triggers explicit reset and full refresh |
| Tenant isolation | Zero cross-tenant events or cached data disclosure under switch/revocation |
| Stream failure recovery | Eventual convergence via replay, reset or bounded fallback; no permanent silently stale state |

These are review targets, not measured outcomes. Determine the coalescing window, heartbeat interval, fallback cadence, replay retention and load envelope through representative benchmarks before normative approval.

## 7. Acceptance-test matrix (proposed)

| Test ID | Requirement | Scenario / pass condition |
| --- | --- | --- |
| PUSH-T01 | 001,004,011 | Committed service-health event refreshes an active service view without timer-driven query polling |
| PUSH-T02 | 003,011 | Deployment/change-marker event is emitted only after the authoritative read sees the new revision |
| PUSH-T03 | 004,005 | Unrelated service event leaves unrelated TanStack Query keys untouched; selection/filters persist |
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
| PUSH-T16 | 011 | Actual browser E2E flow verifies stream → cache invalidation → GET query → visible UI changes |

Tests must be traceable to **one** subrequirement each in the machine-checkable spec; this scenario matrix is the review index, not the final `verifies` annotation map. Add positive, negative, boundary, failure and restart cases in owned test suites before implementation.

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
3. **Subscription facade:** implement `observable-query` SSE transport, cursor, lifecycle, auth revalidation, pressure handling and distributed fan-out, using provider fakes.
4. **Frontend:** implement a single stream manager, query-key mapper, status UI and tenant-switch lifecycle; add programmable playground adapter.
5. **Join & rollout:** browser/full-stack tests, load and resilience exercises, compare against 5-second polling, progressively disable old polling per view, and record requirement-level evidence.
6. **Later scope:** telemetry visibility-watermark updates for metric panels and logs/traces. Separate acceptance gate; not assumed delivered by MVP.

## 10. Validation and approval gate

- `spec/system.yaml` adds only **draft P1/P2** obligations; current P0 release evidence is untouched.
- Run `system-spec spec/system.yaml --impact <id>` on changed capabilities, `--compat` against the previous main spec, `bash scripts/spec-gate.sh`, and scoped mock/contract checks.
- Owner reviews requirements and tests **before** any implementation PR. No new requirement is labeled verified without current criterion-specific evidence.
- Reassess existing ADR-031 (tenant context) only for integration assumptions; this feature introduces **no auth redesign**.
