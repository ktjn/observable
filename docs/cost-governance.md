# Cost governance

Contract of `report-observability-cost` for OBS-17-01 and OBS-17-02, provided by observable-control in `services/admin-service/src/cost_governance.rs`. Tables `control.service_cost_attribution`, `control.cost_budgets` and `control.cost_budget_alerts` (migrations 080, 086).

## Rules

- Every route sits behind `require_tenant`. The tenant comes from the caller's credential; a `tenant_id` in the body or query is ignored.
- Recording spend, saving a budget and evaluating a budget need the `tenant_admin` role. Any member reads the attribution report.
- A budget holds the primary and backup owner and the URL each is warned at. The URLs are checked by the outbound policy of `docs/operational-context-exchange.md` when the budget is saved and again on every send. An evaluate request cannot name a destination.
- Every breach outcome is recorded in `cost_budget_alerts` with the recipient, status and error. A breach that cannot be recorded answers `503`; one that reaches neither owner answers `502`.

## Routes

| Route | Requirement | Behaviour |
| --- | --- | --- |
| `POST /v1/cost/expenditures` | OBS-17-01 | `{service_name, owner_team?, cost_center?, expenditure_cents, period}` |
| `GET /v1/cost/attribution-report?period=` | OBS-17-01 | spend grouped by owner team and cost center; spend without both is listed as unattributed, never distributed |
| `PUT /v1/cost/budgets` | OBS-17-02 | `{cost_scope, budget_cents, primary_owner, backup_owner, primary_url, backup_url}` |
| `POST /v1/cost/budgets/evaluate` | OBS-17-02 | `{cost_scope, actual_cents, reason}`; over budget warns the primary owner, then the backup owner when the primary is unreachable |

## Not covered

- OBS-17-03/04/05/08 (category reconciliation, policy cost comparison, rationalization candidates, retention reduction) have no cost data source yet and remain unit-verified helpers.
- Spend is recorded by a caller; there is no metering pipeline into `service_cost_attribution`.

The [W2b cost evidence proposal](plans/w2b-cost-evidence.md) lists the source decisions and integration checks for these four requirements.
