# Alerting: contractual SLA review, capacity forecasts, recorded procedures

Contract for W3 of [wire-unrouted-capabilities](plans/wire-unrouted-capabilities.md). Provider: `observable-alerting` (alert-evaluator). Storage: migration 088. Every route sits behind `require_tenant`; tenant and actor come from the caller's credential only.

## Contractual SLA review (OBS-06-10, `manage-reliability-objectives`)

| Route | Who | Behaviour |
| --- | --- | --- |
| `PUT /v1/objectives/{id}/contract` | signed-in `tenant_admin` | Records `contract_ref`, `contracted_target` (0..1), `period_fee_minor` (minor units), `currency` (ISO 4217), `credit_tiers` (1-10, ascending `below`, each at most the target, `credit_percent` in (0,100]). Objective must be readable (catalog check, hidden = 404). |
| `GET /v1/objectives/{id}/sla-review?from=&to=` | any reader | Period at most 93 days, not in the future. Approved objective only (409), terms required (404 `contract_not_found`). Returns attainment over stored spans in the period and `obligation {met, credit_percent, credit_minor, currency}`: the most severe tier whose `below` exceeds attainment. |

`basis` is `evidence` only when the period has ended and spans cover it to within the objective's freshness bound at both ends. Otherwise `estimate`, with `basis_reasons` from `period_in_progress`, `evidence_starts_late`, `evidence_ends_early`, `no_evidence`. Without evidence there is no attainment and no obligation.

## Capacity forecast (OBS-07-12, `evaluate-alerts`)

- Create with `POST /v1/admin/alerts/rules` (`tenant_admin`): `alert_type: capacity_forecast`, `metric_name`, `threshold`, `lookback_secs` and `horizon_secs` (each 60 s to 30 days). Opens no incident unless `auto_trigger_incident: true`.
- Each evaluation fits a least-squares line to the metric over the lookback (at least 6 points). It fires only for a rising trend that reaches the threshold within the horizon; it resolves when the trend no longer does.
- The firing's `value` is the predicted value. `forecast` on the firing (`GET /v1/alerts/rules/{id}`) and on the webhook payload carries `label: "forecast"`, `current_value`, `predicted_value`, `seconds_to_threshold`, and `basis {method, lookback_secs, points, slope_per_hour, r_squared}`. A forecast is never presented as a measured breach.

## Recorded procedure execution (OBS-33-05, `manage-operational-guidance`)

The approved procedure of an incident is its runbook, which only a tenant admin sets on the rule.

| Route | Who | Behaviour |
| --- | --- | --- |
| `POST /v1/incidents/{id}/procedures` | signed-in member or admin | `{procedure_ref, automated}`. `procedure_ref` must equal the incident's runbook (409 otherwise). Manual runs start `in_progress`; automated runs start `pending_approval`. |
| `POST .../procedures/{run_id}/approve` | signed-in `tenant_admin`, not the requester | `pending_approval` to `in_progress`. |
| `POST .../procedures/{run_id}/steps` | member or admin; an API key only on an approved automated run | `{name, inputs, outputs, result}`; inputs and outputs are JSON objects up to 16 KiB; result `succeeded`, `failed` or `skipped`. Actor kind and id come from the credential. |
| `POST .../procedures/{run_id}/finish` | signed-in member or admin | `{outcome: completed or failed}`; no steps after. |
| `GET /v1/incidents/{id}/procedures` | any reader | Runs with ordered steps. |

Other tenants get 404 throughout. Contracts, runs and steps are removed by the alerting tenant purge.
