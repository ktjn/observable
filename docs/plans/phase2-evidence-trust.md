# Phase 2 detailed plan: Evidence trust and health

**Status:** in progress. E1–E3 delivered (see section 7 notes); E4 next.
**Inputs:** [business-spec-adoption.md](business-spec-adoption.md) section 5, [phase1-health-provider.md](phase1-health-provider.md), [phase1-exit-review.md](phase1-exit-review.md), ADR-039 (health assessment), ADR-041 (decide-access), ADR-042 (domains), [COMPONENT-ISOLATION.md](../../system-spec/COMPONENT-ISOLATION.md).
**Branch policy:** every slice is one PR into `2.0`, one component per PR where possible, `bash scripts/spec-gate.sh` green, change flow per `system-spec/FEATURE.md`.
**Depends on:** phase 1. Phase 1 is at 9/15 verified, and the six remaining requirements are organization inspections. The ROADMAP says to close phase 1 before the remaining phase 2 scope starts. Q0 asks whether E1 can start while the inspections are pending.

## 1. Scope and exit

Phase 2 must verify 11 P0 requirements. All of them are platform-held, so every requirement is verified by tests, with no inspections.

| ID | Statement (short) | Gap today | What is missing |
|---|---|---|---|
| OBS-03-01 | current health of every critical service | partial | The legacy service pill was removed in C4.3. The catalog record and review show evaluated health. No service overview lists the health of critical services. The gap register entry is stale. |
| OBS-03-02 | distinguish unavailable, degraded, maintenance, no evidence | partial | `platform-default` yields only `normal` or `unknown`. There are no degraded or unavailable conditions and no maintenance records. |
| OBS-03-03 | when a service last had sufficient evidence | partial | `last_evidence_at` and `evidence_status` are returned and shown in the catalog record. The service overview and freshness threshold disclosure are missing. |
| OBS-03-04 | recent and historical interruptions and degradations | partial | Only the current snapshot is stored. There is no transition history, onset, duration or unresolved state. |
| OBS-13-03 | disclose reporting period, scope and freshness | partial | There is a global date picker. Results disclose no freshness and no partial state. |
| OBS-24-01 | five states by text and visual treatment | partial | `HealthLabel` uses text and tone. There is no icon or shape, and no grayscale or screen-reader check. |
| OBS-24-02 | empty, incomplete, delayed, stale, unavailable, restricted differentiated | partial | States are ad hoc per page. Failed queries can render as zero. For example, infrastructure entity `health_state` treats a missing error rate as 0, which is healthy. |
| OBS-26-01 | observations complete, current, fit for decision | missing | There is no fitness disclosure beyond `evidence_status`. |
| OBS-26-02 | missing, inconsistent, unexpectedly absent observations | partial | A deadman alert type exists. No data-quality issue is raised for a bound identity that stops reporting. |
| OBS-26-04 | absence of failures vs absence of information | partial | Health returns `unknown` or `missing`. Telemetry views and incident counts still default to zero. |
| OBS-29-03 | communicate degraded ability to assess, detect, report | missing | Evaluator, alert evaluator and ingest pipeline degradation are invisible to users. |

UX-07 (degraded observability, critical, exhaustive) also references these requirements:

- OBS-24-08 and OBS-24-11 (P1): last updated, auto-refresh, and refresh that does not move the inspection. These are in scope because UX-07 scenarios name them.
- OBS-24-05 (P0, WCAG 2.2 AA, phase 3). Phase 2 views follow it, and phase 3 owns its verification.
- OBS-25-02 (P0, phase 3) and `req-health-access-control` (verified in phase 1 C2.3). These are covered by the restricted-evidence scenario.

**Exit (spec terms):**

- `risk-false-healthy` mitigated. Its five mitigating requirements (03-01, 03-03, 24-02, 26-04, 29-03) are verified.
- All 11 P0 requirements are `verified` under `scripts/spec-readiness.py --priority P0 --strict` for phase 2.
- UX-07 passes. Its six scenarios run in a browser test (`ux-07-success`, `ux-07-failure`, `ux-07-restricted-evidence`, `ux-07-refresh-success`, `ux-07-refresh-failure` and any added by `check-ux-contracts.py`).
- `bash scripts/spec-gate.sh` is green.

**Out of scope:**

- Regions, channels and segments (OBS-03-05).
- Recurring problems (03-06, 26-05).
- Owner-defined health composition (03-07, phase 7 dependencies).
- Shared platform health (03-08). Only its false-healthy default is fixed here.
- Source and confidence in investigations (26-03, phase 6).
- Sampled-evidence disclosure beyond the health rule (26-06).
- The experience shell, scope bar and navigation (phase 3).

## 2. Component placement

| Item | Decision |
|---|---|
| Assessment | `observable-health` (domain `service-assessment`). It extends `assess-service-health` with conditions, maintenance, history and data-quality issues. It owns `health-assessment` and adds `maintenance-window`, `health-transition` and `data-quality-issue` (owns_data stays at most 4, COMP-03). |
| Detection status | `observable-alerting` reports its own evaluator status. Health must not call alerting: `incident-response` uses `service-assessment`, so a call the other way is a cycle (COMP-06). |
| Ingest and storage freshness | `store-telemetry` exposes a per-tenant ingest watermark (latest stored timestamp per signal). Health and alerting read it through the existing `store-telemetry` requirement. |
| Composition | `observable-query` (application) composes health, the data-quality issues and each provider's self-status into the overview and review responses. It already requires `assess-service-health`, `manage-reliability-objectives` and `manage-service-catalog`. |
| Web | `observable-web` gets a shared state vocabulary (`StateLabel`, `FreshnessNote`, `DataState`), a service overview and the service detail health panel. It consumes the providers only through query and the runtime contract. |

ADR-042 allows each edge: `investigation` uses `service-assessment` and `incident-response`, and `service-assessment` uses `telemetry-storage`. No new domain edge is needed.

## 3. Data model (`health` schema, all rows carry `tenant_id`)

| Model | Key fields | Requirements |
|---|---|---|
| `health-rule` (exists) | rule_id, revision, tenant_id (null = platform default), freshness_seconds, lookback_seconds, conditions[], published_by, published_at | 03-02 |
| rule condition (exists, extended) | kind (`presence`, `error_ratio`, `latency_quantile`, `request_absence`), signal, threshold, window, yields (`degraded` or `unavailable`), required | 03-02, 26-01 |
| `maintenance-window` | service_id, environment_class, starts_at, ends_at, reason, created_by, approved_by/at, cancelled_at | 03-02 |
| `health-transition` | service_id, environment_class, from_state, to_state, evidence_status, at, rule revision, evaluation id | 03-04 |
| `data-quality-issue` | service_id, environment_class, identity (service_name, environment), kind (`source_stopped`, `inconsistent_inputs`, `evaluation_failed`), opened_at, last_seen_at, resolved_at | 26-02, 26-04 |

An **interruption** is derived from transitions, not stored separately:

- It starts at a transition into `degraded` or `unavailable`.
- It ends at the next transition back to `normal`.
- A transition to `unknown` or `stale` does not end it. The interruption stays open with an unknown end, which is the 03-04 negative criterion.

Data models go in `models/health.mdl` with generated Rust and TypeScript. Never hand-edit `generated/`.

## 4. Contracts

| Capability | Operations (sketch) | Failure modes |
|---|---|---|
| `assess-service-health` 0.2.0 | Existing: `GET /v1/health/services/{id}`. Add `GET /v1/health/services/{id}/history?environment_class=&from=&to=` (transitions and interruptions), `GET /v1/health/issues?environment_class=` (open data-quality issues for visible services), and tenant rule writes `PUT /v1/health/rules/{id}` (new revision, audited). Maintenance: `POST/GET /v1/health/services/{id}/maintenance`, `POST .../{window}/approve`, `POST .../{window}/cancel`. Internal: `GET /internal/health/self-status`. | existing set plus `rule_invalid`, `maintenance_unapproved`; restricted history follows the restricted-unknown rule |
| `evaluate-alerts` (alerting) | `GET /internal/alerting/self-status`: last completed evaluation pass, rules evaluated, failures, lag | `self_status_unavailable` |
| `store-telemetry` | Read-only watermark query: max ingest time per tenant and signal (`spans`, `logs`, `metric_points`) | unavailable is reported as unknown confidence, never as current |
| `query-telemetry` (query) | `GET /v1/review/services` gains `period`, `scope`, `freshness`, `confidence` and open issue counts. `GET /v1/overview/services?environment_class=` lists critical services with state, evidence age and issues. `GET /v1/platform/confidence` composes the self-statuses. | a provider outage becomes `confidence: degraded` with the named function, not an error page and not healthy |

Bump each contract version with the `--compat` rules. Add the interface blocks to the capabilities before implementation (FEATURE.md step 2).

## 5. State semantics (the rules every slice keeps)

1. **No default to healthy.** A missing, failed, partial or restricted input never yields `normal`, zero errors or zero incidents. It yields `unknown` with a reason. This is the shared invariant `missing_evidence_never_shown_as_healthy`.
2. **Affirmative evidence for bad states.** `unavailable` requires a condition that proves loss of service, such as requests present with every request failing, or a bound identity with request absence while upstream traffic exists. `degraded` requires a breached `error_ratio` or `latency_quantile`. Missing data is never `unavailable`.
3. **Maintenance requires an approved, current window.** An unapproved or expired window has no effect. During a window, failing evidence is shown as `planned_maintenance`, with the underlying condition visible to authorized users.
4. **Freshness is judged at read time.** Freshness and fitness come from the rule's freshness bound. A stale snapshot is `unknown` with `stale` and its last-evidence time, so an evaluator outage cannot keep a service `normal`.
5. **Fitness.** A view is fit for a decision only when evidence is `current` for every required condition and platform confidence is `full`. Otherwise it states what is missing (OBS-26-01).
6. **Platform confidence.** Confidence is `full`, `degraded` (with the affected function: assess, detect, report, ingest) or `unknown`, computed in query from the self-statuses and watermarks:
   - Health: evaluation pass older than 2x its interval.
   - Alerting: evaluation pass older than 2x its interval.
   - Ingest: tenant watermark older than the agreed bound.

   Any view whose evidence depends on a degraded function shows a loss-of-confidence warning (OBS-29-03).
7. **Data-quality issue.**
   - An identity that had evidence within the lookback and then has none for one freshness bound opens a `source_stopped` issue. The issue resolves when evidence returns.
   - Ambiguous catalog inputs open `inconsistent_inputs`.
   - Issues are disclosed only for visible services, with the same restricted rule as evidence.

## 6. Isolation and authorization

- All health reads keep the C2.3 order:
  1. credential
  2. class
  3. `decide-access` on the resource
  4. the caller's catalog read
  5. evidence contributors

  History, maintenance and issues add their own resource types (`health.history`, `health.maintenance`, `health.issue`) with grants in a new auth migration.
- Rule and maintenance writes require `tenant_admin` or the service's operational owner, checked through the catalog. Every write records actor and time. Approval needs a second person, and approving your own request is refused.
- Self-status endpoints are internal (internal token). They carry no tenant data except counts, and the per-tenant watermark is read only for the caller's tenant.
- Tests run as the scoped logins (`observable_health`, `observable_alerting`), following the phase 1 lesson that the owner URL hides grant gaps.

## 7. Slices

| Slice | Component | Deliverable | Evidence to run |
|---|---|---|---|
| E1 | health | Rule conditions `error_ratio`, `latency_quantile` and `request_absence`. Tenant rule revisions with audit. `degraded` and `unavailable` per section 5.2. | Induced error ratio gives `degraded`; total failure gives `unavailable`; no data gives `unknown` (never `unavailable`); sampled-only evidence is limited as the rule says. Run against real ClickHouse through the read-only session. (03-02 partial) |
| E2 | health | Maintenance windows with second-person approval. `planned_maintenance` state. | Approved current window gives maintenance; unapproved, expired and cancelled windows have no effect; self-approval is refused. (03-02) |
| E3 | health | Transitions and derived interruptions, plus the history read. | Known outage located with onset, duration and resolved; missing end shown as unresolved or unknown duration; restricted history gives the restricted response. (03-04) |
| E4 | health | Data-quality issues, issue read, health self-status. | Source stops: issue raised within one freshness bound and resolved when evidence returns. Inconsistent inputs raise an issue. The worker stopped is visible in self-status. (26-02, 26-04 backend) |
| E5 | alerting, store-clickhouse | Alerting self-status and the ingest watermark read. | Evaluator paused gives a stale pass; watermark per tenant; another tenant's watermark is not readable. (29-03 inputs) |
| E6 | query | Overview and review carry period, scope, freshness, confidence and issues. `GET /v1/platform/confidence`. Fix infrastructure `health_state` defaulting a missing error rate to healthy. | Each provider outage becomes degraded confidence naming the function; no response renders a failed source as zero or healthy. (13-03, 26-01, 26-04, 29-03, 24-02 backend) |
| E7 | web | Shared state vocabulary: `StateLabel` (text, icon and shape per state, accessible name), `FreshnessNote` (period, scope, last updated, auto-refresh), `DataState` (empty, incomplete, delayed, stale, unavailable, restricted). Service overview and detail health panel. Refresh that keeps selection, sort and filters, with a change notice. | Grayscale and screen-reader checks for all five states; failed source never shows zero; partial chart says partial; refresh keeps the inspection. Vitest and Playwright. (24-01, 24-02, 13-03, 03-01, 03-03, 24-08, 24-11) |
| E8 | spec | UX-07 browser scenarios, evidence records, gap register refresh, `risk-false-healthy` mitigation and the exit review. | Strict readiness 11/11 for phase 2; UX-07 six scenarios pass; spec gate green. |

**E1 delivered notes.**

- Delivered: `error_ratio` and `latency_quantile` conditions over server spans (`span_kind = 'SERVER'`, `status_code = 'ERROR'`), `required`, `min_requests` with an `insufficient` outcome and evidence status, `GET`/`PUT /v1/health/rules` (migrations 066 and 067).
- Assumed (Q2 unanswered): only `tenant_admin` publishes rules, through `decide-access` write on `health.rule`, and only from a user session so the revision names its publisher.
- Deferred: `request_absence` needs upstream topology to prove absence while traffic exists, so it moves to phase 7 with the dependency map. Until then `unavailable` comes only from an `error_ratio` condition with threshold 1 (every request failing).
- Deferred: sampled-only evidence limits. Spans carry no sampling ratio today, so the rule cannot state how sampled evidence is limited. Tracked as an E4 data-quality input.

**E2 delivered notes.**

- Delivered: maintenance windows with request, second-person approval and cancellation, each recording actor and time (migrations 068 and 069). Failing evidence during an approved current window is `planned_maintenance` with `underlying_state`.
- Assumed (Q1 unanswered): health owns maintenance windows.
- Assumed (Q2 unanswered): tenant_admin or the service's operational owner requests and approves, and the approver must be a different person. Approval needs the same rights as a request.
- Limited: the operational owner is recognized only when the owner is a user. A team owner's members cannot manage windows yet; tenant_admin can. Resolving team membership needs `lookup-principals` in health or an ownership answer from the catalog read.
- `unknown` is never turned into `planned_maintenance`: no evidence is not failing evidence (section 5.1).

**E3 delivered notes.**

- Delivered: transitions per state change, an `unknown` transition when evaluation stops for longer than the freshness bound, derived interruptions with exact or bounded onset and end, the history read under `health.history` (migrations 070 and 071), retention (`HEALTH_HISTORY_RETENTION_DAYS`, default 400) and tenant purge.
- Transitions store the assessed state. Maintenance is applied when disclosing the current state, so history shows the underlying state during a window; marking interruptions inside approved windows is left to E6, where overviews compose both.

E1–E4 are sequential within health. E5 can run in parallel with E1–E4. E6 needs E3–E5, E7 needs E6, and E8 runs last.

## 8. Risks

| Risk | Treatment |
|---|---|
| Rules that call missing data `unavailable` reintroduce false states the other way. | Section 5.2 plus an E1 negative test that fails any rule yielding `unavailable` from absence. |
| Confidence thresholds fire on normal jitter and train users to ignore warnings. | Thresholds are 2x the worker interval and are configurable. E6 records how often the warning fires in the local stack. |
| Self-status endpoints leak cross-tenant activity. | Counts only. The watermark is per caller tenant. Tests cover a second tenant. |
| History growth. | Transitions only (not every evaluation), with a retention setting. A purge step is added to the tenant purge feed in E3. |
| Web state vocabulary diverges per page. | One component set in E7. A lint test forbids raw `?? 0` on fetched counts in service views. |

## 9. Open questions

| # | Question | Recommendation |
|---|---|---|
| Q0 | Can phase 2 slices start while the six phase 1 inspections are pending? | Yes. No phase 1 platform work remains, and the inspections need deployment evidence, not code. Keep the phase 1 exit review as a precondition for the phase 2 exit review. |
| Q1 | Who owns maintenance windows: health, or control (as tenant configuration)? | Health. Maintenance changes how evidence is assessed, and keeping it with the assessment avoids a new control-to-health read. |
| Q2 | Who may publish tenant health rules and approve maintenance? | Rules: `tenant_admin`. Maintenance: the operational owner or `tenant_admin` requests it, and a different person with the same rights approves. |
| Q3 | What is the "agreed period" for a data-quality issue and for platform confidence? | The rule's freshness bound for issues (default 300 s). 2x the worker interval for confidence. Ingest watermark bound 300 s, configurable per deployment. |
| Q4 | Do the P1 refresh requirements (24-08, 24-11) enter phase 2 because UX-07 names them? | Yes, for the service overview and detail only. Other views follow in phase 3. |
| Q5 | Should the stale gap-register evidence (OBS-03-01 still cites the removed pill) be corrected now or in E8? | Now, in the PR that merges this plan, so readiness reflects C4.3. |
