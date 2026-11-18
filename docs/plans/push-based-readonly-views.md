# Feature proposal: push-based updates for read-only views

**Status:** MVP verified (Waves 0–5 complete: `PUSH-001`–`PUSH-015`, `PUSH-017`–`PUSH-020`); Wave 6 P2 slices (`PUSH-016`, `PUSH-021`–`PUSH-023`) remain proposed.
**Baseline:** `main` after auth tenant-selection change (PRs #945 and #946).
**Owner:** Observable platform, `observable-query` + `observable-web` and domain event providers.
**Scope:** Browser operational read-only views. No changes to tenant selection or authentication APIs.

**Design review (2026-11-15):** this plan was grilled and simplified before implementation. MVP keeps data-bearing SSE but cuts: the `subscriptionId` POST/PUT/DELETE lifecycle (collapsed into one GET call), the durable per-tab lease/registry (replaced with ephemeral in-memory per-connection context), resume via full replay history (replaced with a small in-memory event buffer, falling back to reset), and the entire adaptive flow-control/lag-skip/ACK subsystem (deferred to a P2 follow-on slice, `PUSH-021`–`PUSH-023`). The security prerequisite (§7) is now tracked as its own gating ticket, not a footnote. Sections below are updated to reflect this; superseded text is removed rather than kept as dead alternatives.

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

Native same-origin `EventSource` uses the existing `HttpOnly` session cookie. The server retains **one in-memory context per connection** (one per browser tab), multiplexing its active read-only views. Context is never an authorization authority: current tenant, identity, role and permitted resources are resolved from the stored session and checked on query execution and each data delivery.

### 3.1 Registration lifecycle

Registration and streaming are a **single call** — there is no separate create/update/delete lifecycle or opaque subscription ID:

```http
GET /v1/ui/subscriptions/events?views=%5B%7B%22viewId%22%3A%22health%22%2C%22query%22%3A%7B%22kind%22%3A%22service-health%22%2C%22service%22%3A%22payments%22%7D%2C%22timeRange%22%3A%7B%22mode%22%3A%22rolling%22%2C%22window%22%3A%2215m%22%7D%7D%5D
Cookie: session=<jwt>
Accept: text/event-stream
```

(`views` is the URL-encoded JSON view-definition array; `EventSource` only issues `GET` with no custom body, so view definitions travel in the query string.)

To change filters, time range or the set of views, the client closes the `EventSource` and opens a new one with updated `views`. Only bounded, allowlisted typed queries are accepted. The stream endpoint enforces same-origin checks; there is no POST/PUT/DELETE surface requiring CSRF protection.

### 3.2 Tracked context

```typescript
interface ViewSubscriptionContext {
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
  replayBuffer: RingBuffer<{ id: string; event: SseEvent }>;
}
```

This context lives **only in the memory of the replica holding the connection**, for the connection's lifetime. There is no subscription ID, no durable registry, and no lease to expire — the connection closing *is* the expiry. The server uses it to resolve relevant events for this connection and enforce per-connection quotas. Distinct tabs are distinct connections with independent contexts. Losing the connection (reload, replica restart, network drop) simply means the client reconnects with its view definitions and gets a fresh authorized snapshot; there is no cross-replica state to recover.

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

- **snapshot:** full authorized initial view result, with event observation established first to avoid the snapshot/subscribe race.
- **delta:** a pass-through of an already-known upsert/delete op taken directly from the domain event. No server-side diffing or revision-chain bookkeeping; if the event doesn't already carry a trivial op, use `replace` instead.
- **replace:** complete fresh result for smaller aggregate or state views; not a separate query engine.
- **invalidate:** fallback hint requesting normal query API refetch for unsupported or expensive delta projections.
- **reset:** replay-buffer miss, changed permissions, connection-queue overflow, restart, or incompatible schema; client re-registers (reopens the `EventSource` with its view definitions) and gets an authorized snapshot.
- Events are delivered at least once. Every data event is schema- and size-bounded and tagged by view, query revision and result revision. Client rejects duplicates, stale revisions and results for replaced view definitions. `Last-Event-ID` resumes only when it is still present in the connection's small in-memory replay buffer and session/tenant/scope/query revision all match; otherwise `reset`.
- SSE comments provide heartbeats. Enforce `Cache-Control: no-store`, streaming proxy configuration and no buffering. Never put JWTs, raw session secrets or client tenant selectors in the event URL.

### 3.4 MVP backpressure

Each connection has a **fixed-size queue**. A successful transport write is the only scheduling signal used — there is no application-level ACK and no attempt to distinguish "written to the socket" from "applied by the browser" in MVP. On overflow, the server disconnects that connection; the client reconnects and receives a fresh authorized snapshot. No conflation, no per-view fair scheduling, no partial `supersedesThrough` replacement. This is a deliberately simple floor, not the full flow-control design — see §3.4.1.

**Operational visibility:** count connection disconnects due to queue overflow, queue utilization, reset reasons and recovery latency using low-cardinality metrics.

#### 3.4.1 Deferred: adaptive flow control, lag skipping and ACK (P2, later slice — `PUSH-021`–`PUSH-023`)

Not built for MVP. Recorded here so the later slice starts from an agreed design rather than re-deriving it:

Read-only view subscriptions should eventually prioritize **convergence to current authorized state** over delivery of every intermediate transition, with flow control **per view** (a slow metric panel must not block health/alert updates sharing the connection). Overload policy: emit deltas while bounded; on soft threshold conflate pending changes by semantic key; on hard threshold emit a complete `replace` with an explicit `supersedesThrough` revision boundary and resume deltas only after it; if no complete replacement is possible, `reset`. An optional per-view `POST /v1/ui/subscriptions/{id}/ack` would report the highest fully **applied** revision as advisory scheduling telemetry — batched, never per event, never authorization evidence, never blocking rendering on failure.

```text
event: replace
data: {"v":1,"viewId":"health","queryRevision":"r1","revision":"1850","supersedesThrough":"1849","reason":"subscriber_lag","data":{"service":"payments","health":"degraded"}}
```

Do not implement this against MVP's simple fixed-size queue; it replaces that mechanism when the slice is scheduled.

### 3.5 Correctness, authorization and recovery

1. On the registering request, authorize each query using the existing query facade, then establish change observation and produce the initial snapshot or reconcile any changes during snapshot execution.
2. Domain publishers emit versioned change events only after their state is query-visible. The subscription manager selects matching connections' registered views and performs an authorized query, passes through a trivial typed delta op, or emits an authoritative replacement.
3. The client applies data events directly to its typed view state or TanStack Query cache. There is **no mandatory second GET** after a data event; invalidation falls back to GET only when required.
4. For rolling ranges, expire old buckets independently of new events; fixed historical views do not subscribe.
5. Session tenant switching, logout, revocation and membership/role changes terminate old streams. Server revalidates authoritative session and resource permission **before emitting any sensitive data batch**, even when switching occurs in a different tab. Identity-service outage fails closed.
6. Enforce per-connection quotas on memory, queued results, emitted bytes, query execution and reconnects via the fixed-size queue (§3.4); overflow disconnects and forces re-registration, no conflation in MVP.
7. On SSE outage or background-tab return, reconcile by snapshot or bounded fallback polling and show live/reconnecting/stale/paused/unauthorized state.
8. Server tracks active connection count, view count, delivered payload bytes, backpressure disconnects, reset count and authorization failures through safe low-cardinality observability labels.

## 4. Component handoff obligations

`PUSH-001` is the parent platform requirement. Every provider-specific subrequirement must refine `PUSH-001` in `spec/system.yaml`; parent integration verification is separate.

| ID | Owner | Obligation | Priority |
| --- | --- | --- | --- |
| PUSH-001 | platform integration | Server-managed read-only views receive authorized snapshots and incremental/replacement data without fixed query polling | P1 |
| PUSH-002 | observable-query | Provide authenticated single-call registration, per-connection in-memory client context and buffer-resumable data-bearing SSE | P1 |
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
| PUSH-017 | observable-query | Track per-connection in-memory context, query revisions and replay-buffer state for the connection's lifetime | P1 |
| PUSH-018 | observable-query | Deliver authorized snapshot/delta/replace data; delta only as pass-through of an already-known op; invalidate only when appropriate | P1 |
| PUSH-019 | observable-web | Apply SSE result revisions idempotently and reject obsolete view-query revisions | P1 |
| PUSH-020 | observable-query | Bound each connection with a fixed-size queue; disconnect on overflow | P1 |
| PUSH-021 | observable-query | *(deferred, P2)* Automatically skip superseded intermediate state for lagging subscribers and deliver the latest complete replacement or reset | P2 |
| PUSH-022 | observable-query | *(deferred, P2)* Validate optional session-bound batched applied-revision ACKs; reject stale, future, mismatched or unauthorized revisions | P2 |
| PUSH-023 | observable-web | *(deferred, P2)* Report optional batched fully applied client revisions without blocking rendering | P2 |
| PUSH-016 | observable-store-clickhouse + query (future slice) | Publish/coalesce query-visible telemetry watermarks for metric/log/trace streaming | P2 |

No new auth capability is required. `observable-auth` remains the identity/authorization dependency; do not add cross-owner database queries, JWT claims, a new tenant header, or a second tenant-switch API.

**Context budgets:** Adding the SSE façade and provider publication *capabilities* (`subscribe-readonly-changes`, `present-pushed-readonly-views`, `publish-*-readonly-changes`) still increases `observable-web`, `observable-query`, `observable-control` and `observable-alerting` beyond the default `max_provides: 5` limit; query additionally reaches 10 required capabilities. The proposed `spec/system.yaml` explicitly sets scoped overrides (`max_provides: 6`, and `max_requires: 10` for query). This is unchanged by the MVP simplification below: cutting the `subscriptionId` lifecycle, ACK endpoint and flow-control subsystem reduces the *requirement* count (`PUSH-017`/`020`–`023`) and their test surface, but the *capability* count — what the budget actually limits — is the same, since `subscribe-readonly-changes` and `present-pushed-readonly-views` are still new capabilities either way. Review module extraction if these counts continue to grow.

### Mock contracts and independence

- Provider-owned programmable mocks (control, health, alerting) must generate committed, duplicate, delayed, unauthorized and unavailable event sequences. The query provider publishes a programmable SSE fake with register-and-stream (single call), snapshot/delta/replace/invalidate/reset, open/close/retry/gap/overflow and auth-switch scenarios. No register/update/delete-lifecycle scenarios in MVP — there is no separate lifecycle to fake.
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
| Backpressure (MVP) | Fixed-size per-connection queue; overflow disconnects, client reconnects to a fresh snapshot — no conflation/replacement skip in MVP |
| Lag measurement (MVP) | Transport write success only; no ACK in MVP |
| Recovery correctness | Buffer-backed `Last-Event-ID` resume within the connection's replay window; anything else is `reset` + fresh snapshot |
| Tenant isolation | Zero cross-tenant events or cached data disclosure under switch/revocation |
| Stream failure recovery | Eventual convergence via buffer resume, reset or bounded fallback; no permanent silently stale state |

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

| PUSH-T17 | 017 | A single GET call registers views and streams; disconnect/reload ends the in-memory context with no durable state, and reconnect gets a fresh snapshot regardless of which replica handles it |
| PUSH-T18 | 018,019 | Duplicate, out-of-order, stale query revision and oversized data events are rejected or reset without corrupting cache |
| PUSH-T19 (MVP) | 020 | A stalled/slow consumer's fixed-size queue hits its bound, the connection is disconnected, and the client reconnects to a fresh authorized snapshot — bounded memory, no starvation of other connections |

Tests must be traceable to **one** subrequirement each in the machine-checkable spec; this scenario matrix is the review index, not the final `verifies` annotation map. Add positive, negative, boundary, failure and restart cases in owned test suites before implementation.

### Deferred acceptance tests (P2, with `PUSH-021`–`PUSH-023`)

Not written against MVP. For the later flow-control slice:

| Test ID | Requirement | Scenario / pass condition |
| --- | --- | --- |
| PUSH-T20 | 021 | Lagged subscriber skips superseded deltas, receives the current authorized replacement and resumes subsequent deltas without a revision gap |
| PUSH-T21 | 021 | An append-only/non-replaceable view never silently drops history: missing replacement results in reset and authoritative resync |
| PUSH-T22 | 022 | Server rejects malicious, stale, future, nonmonotonic, cross-tenant and old-query-revision ACKs; failed ACKs cannot affect authorization or SSE delivery |
| PUSH-T23 | 020,021,022,023 | Simulated high-throughput events and a stalled browser prove bounded work, explicit skipped-count metrics and eventual convergence |
| PUSH-T24 | 023 | Browser emits bounded batched ACKs only after applying the current view revision; ACK failure does not block rendering |

### Existing authorization prerequisite — own gating ticket, not a footnote

Current `auth-service/src/oidc.rs::resolve_session` reads `user_tenant_roles` with a left join and can fall back to the JWT's previous `role` when the active membership row is absent. An SSE connection must not treat that fallback as current authorization. This is tracked as its own independently reviewed authentication/security fix, landed as a separate PR **before** wave 3 (subscription facade) starts — not bundled into a push-feature PR. This does not authorize new tenant-selection mechanisms or change the scope of the header-removal task.

## 8. Risks

| Risk | Mitigation and evidence |
| --- | --- |
| Stream survives tenant switch or revocation | Server-side session tenant revalidation before emission, independent of client tab cleanup; negative browser and backend tests |
| Missed events or delayed broker write | In-memory replay buffer where available, reset on uncertainty, fall back to authoritative reads; reconnect and storage outage tests |
| Push-before-query-visibility race | Commit/visible-watermark publication; query replay tests |
| Query storm from telemetry updates | Materialization budgets and bounded coalesced query execution; workload measurements. Fair per-view scheduling and latest-state replacement are deferred to the P2 flow-control slice |
| Slow browser creates unbounded queued events | MVP: fixed-size per-connection queue, transport-write deadlines, disconnect-and-reconnect on overflow. Per-view fairness and safe skip-to-latest are deferred (P2) |
| Skipping breaks delta chain | Not applicable to MVP (no skip logic yet); when the P2 slice adds it, a complete revisioned replacement supersedes skipped updates, with reset as fallback for non-replaceable views |
| Client drift during rolling window | Separate local clock advancement from network invalidation; fixed-range tests |
| Proxy-buffered or silently dropped SSE | Same-origin distribution smoke, no-store and heartbeat checks, explicit stale-status detection |
| Business/tenant event metadata leakage | Sanitized hints and per-resource access checks; existence-hiding tests |

## 9. Implementation waves — only after approval

0. **Security prerequisite:** land the `resolve_session` fail-closed authorization fix as its own reviewed PR, independent of this feature. Gates wave 3.
1. **Contract review:** approve requirements, subrequirements, failure modes, acceptance cases, API schema, ADR, and component impact with `system-spec --impact` / `--compat`.
2. **Provider-first:** independently implement control, health and alerting event publication with programmable mocks and real-provider contract suites.
3. **Subscription facade:** implement `observable-query`'s single-call register-and-stream endpoint, per-connection in-memory context, authorized snapshot/delta/replace delivery, replay buffer, auth revalidation and the fixed-size-queue backpressure floor, using provider fakes. Depends on wave 0.
4. **Frontend:** implement a single stream manager, view registration on the stream request, revision-aware SSE result application, fallback invalidation, status UI and tenant-switch lifecycle; add programmable playground adapter.
5. **Join & rollout:** browser/full-stack tests, load and resilience exercises, compare against 5-second polling, progressively disable old polling per view, and record requirement-level evidence.
6. **Later scope (separate acceptance gate, not assumed delivered by MVP):** telemetry visibility-watermark updates for metric panels and logs/traces (`PUSH-016`); adaptive per-view flow control, lag-skip replacement and ACK (`PUSH-021`–`PUSH-023`).

## 10. Validation and approval gate

- `spec/system.yaml` adds only **draft P1/P2** obligations; current P0 release evidence is untouched.
- Run `system-spec spec/system.yaml --impact <id>` on changed capabilities, `--compat` against the previous main spec, `bash scripts/spec-gate.sh`, and scoped mock/contract checks.
- Owner reviews requirements and tests **before** any implementation PR. No new requirement is labeled verified without current criterion-specific evidence.
- Reassess existing ADR-031 (tenant context) only for integration assumptions; this feature introduces **no auth redesign**.
