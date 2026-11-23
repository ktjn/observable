# Wire unrouted 2.0 capabilities

**Status:** proposed (FEATURE.md Phase 1, awaiting requester approval). **Requested by:** Tommy, 2026-11-23 ("wire them in").
**Baseline:** `main` `0c59192`, `spec/system.yaml`, observability-spec v2.7.3.

## Problem

88 requirements are `verified` by unit tests in modules that no route, worker or caller reaches. Some tests only assert a struct literal against itself (`context_exchange.rs::obs_30_p1_external_communication_checks`). Some modules sit in a component that does not provide the capability they claim (`query-api/src/p2_assurance.rs` claims control, auth and response capabilities).

| Module | Component today | Requirements | Capability (provider per spec) |
| --- | --- | --- | --- |
| `services/query-api/src/ops_assistant.rs` | query | OBS-18-01..10, OBS-38-01..28, OBS-19-07 (39) | `assist-operations` (query), `observable-platform` |
| `services/alert-evaluator/src/guidance_graph.rs` | alerting | OBS-39-01..20 (20) | `manage-operational-guidance` (alerting) |
| `services/admin-service/src/context_exchange.rs` p1 tests | response | OBS-16-03/04/07/08/09/10, OBS-30-01..04 (10) | `exchange-operational-context` (response), `communicate-incident-status` (**no provider**) |
| `services/query-api/src/ai_observability.rs` | query | OBS-35-01..08 (8) | `observe-ai-services` (query) |
| `services/query-api/src/p2_assurance.rs` | query | OBS-09-07, 11-05, 11-08, 11-09, 13-07, 15-06, 16-06, 17-06 (8) | `support-investigation` (query); `track-changes`, `report-service-status`, `report-observability-cost` (control); `audit-and-compliance` (auth); `exchange-operational-context` (response) |
| `services/alert-evaluator/src/p2_alerting_assurance.rs` | alerting | OBS-06-10, 07-12, 33-05 (3) | `manage-reliability-objectives`, `evaluate-alerts`, `manage-operational-guidance` (alerting) |

## Rules for every slice

1. **Contract first.** Add the capability `interface` (operations, routes, error mapping), new data models and failure modes to the spec before code. Run `--impact <capability>` and `--compat` against `main`; record both in the PR.
2. **Provider owns it.** Code lives in the `source_paths` of the component that provides the capability. Logic claimed by another component's capability moves there (no cross-component evidence).
3. **Reachable path.** Each requirement is served by a mounted route or a running worker in the provider's binary, behind `validate-credential` and, for restricted reads, `decide-access` (identical deny for restricted and nonexistent).
4. **Durable state.** Records the requirements call audit, history, approval or review are stored in the provider's Postgres schema with a numbered migration and grants. No in-memory store in production paths.
5. **Integration evidence.** Each criterion is bound to a test under the component's `test_paths` that drives the HTTP route (or worker) against Testcontainers Postgres/ClickHouse. A unit test may be added evidence, never the only evidence.
6. **Supersede, don't relabel.** Old unit-only records stay until the slice lands, then are replaced in the same PR. Requirement stays `verified` only if every criterion has integration evidence; otherwise it goes to `specified`.
7. **Gate.** `bash scripts/spec-gate.sh`, `bash scripts/check-component-boundaries.sh` and the component's tests pass. Delete the old module once nothing references it.

## Slices

Ordered smallest-risk first. Each slice is one PR.

| Slice | Provider | Scope | State | Routes (proposed) |
| --- | --- | --- | --- | --- |
| W1 | query | OBS-35-01..08 `observe-ai-services` | stateless over ClickHouse `gen_ai.*` spans and feedback logs | `GET /v1/ai/services/{service}/quality`, `/consumption`, `/instruction-changes`, `/topics`, `GET /v1/ai/interactions/{trace_id}` (reconstruction, goal outcome, feedback link), `GET /v1/ai/safety-probes` |
| W2 | response | OBS-30-01..04 (new provider for `communicate-incident-status`), OBS-16-03/04/07/08/09/10, OBS-16-06 | new data model `incident-communication` (response), Postgres schema `response` | `POST/GET /v1/incidents/{id}/communications`, `POST .../{cid}/approve`, `POST .../{cid}/release`; existing `/v1/context-exchange/*` plus status read, channel dispatch, shared incident view, objective export |
| W3 | alerting | OBS-06-10, OBS-07-12, OBS-33-05 | existing `service-objective`, `alert-rule`; procedure runs added to `incident-record` | `GET /v1/objectives/{id}/sla-review`, alert type `capacity_forecast`, `POST/GET /v1/incidents/{id}/procedures` |
| W4 | alerting | OBS-39-01..20 `manage-operational-guidance` | reads catalog (owners, dependencies) and control (deployment markers); stores ownership history and relationship reviews | `GET /v1/guidance/graph?at=`, `GET /v1/guidance/entities/{id}/context`, `POST /v1/guidance/relationships/{id}/reject` |
| W5 | control, auth, response, query | p2_assurance relocations: OBS-11-05/08/09, 13-07, 17-06 (control), 15-06 (auth), 16-06 (response, folded into W2 if earlier), 09-07 (query) | provider schemas | per capability, defined in the slice contract |
| W6 | query | OBS-18-01..10, OBS-38-01..28, OBS-19-07 `assist-operations` | investigations, hypotheses, proposals, approvals, action audit | `POST/GET /v1/assist/investigations`, `POST .../{id}/pause|resume|cancel|takeover`, `POST /v1/assist/proposals`, `POST .../{id}/approve|execute`, `GET /v1/assist/audit`, `GET /v1/assist/review` |

## Open decisions

| # | Question | Recommendation |
| --- | --- | --- |
| D1 | Where does stateful `assist-operations` data (W6) live? `observable-query` owns no data and has no Postgres. | Give query its own Postgres schema and role with data models `assist-investigation` and `assist-action-audit`. Keeps the spec provider; alternative is moving the capability to alerting, which is at `max_owns_data` 4. |
| D2 | Alerting is at `max_owns_data` 4. W4 needs ownership history and relationship reviews. | Per-component `context_budget.max_owns_data: 6` override with a reason, recorded as model feedback; alternative is folding into `incident-record`, which hides the data. |
| D3 | `observable-response` has its source inside `services/admin-service`, which is also control's `source_paths`. | Keep the shared binary for now; list response paths explicitly and exclude them from control in the boundary script. Split into its own service only if W2 needs a separate role. |

## Verification per slice

- Tests listed in the PR before implementation, one per criterion, including the negative criterion.
- `cargo test -p <service> --test <slice>_integration` (Testcontainers; start `dockerd` in the cloud container).
- `bash scripts/spec-gate.sh` green; readiness unchanged or higher.
