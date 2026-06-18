# P12-S3: Deadman Alert Type — Design

Status: Approved
Date: 2026-06-18
Roadmap reference: `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md` §P12-S3

## Problem

"Is my service silent?" is currently unanswerable inside Observable. The existing alert types
(`threshold`, `slo_burn_rate`, `composite`) all evaluate metric or SLO data; none detect the
absence of telemetry. An SRE has no way to be alerted when a service stops emitting spans
entirely (crash, misconfiguration, network partition).

## Goal

Add a `deadman` alert rule type: fires when no span has been received for a given service
within a configurable window, using the same rule/firing/notification machinery as the existing
alert types.

## Non-goals

- Extending the modelable-generated `AlertRuleItem` schema. Deadman conditions are mapped onto
  the existing `metric_name`/`operator`/`threshold` fields for list/display purposes instead of
  adding new generated fields. (Decision: reuse existing fields, not a schema extension — avoids
  a dependency on an external `modelable` checkout for this slice.)
- Per-environment scoping. The condition only carries `service_name`, matching the acceptance
  criteria in the roadmap plan. Environment-scoped deadman rules can be added later if needed.
- Editing an existing rule's type after creation (no rule type stays fixed at creation time,
  consistent with the other alert types).

## Design

### Backend evaluator (`services/alert-evaluator/src/evaluator.rs`)

- `DeadmanCondition { service_name: String, window_secs: i64 }` — deserialized from
  `alert_rules.condition` JSONB, same pattern as `ThresholdCondition` / `SloBurnRateCondition`.
- `eval_deadman_rules(db: &PgPool, ch: &clickhouse::Client) -> anyhow::Result<()>`:
  1. Fetch rules where `alert_type = 'deadman' AND silenced = false`.
  2. For each rule, parse `DeadmanCondition`; skip with a `tracing::warn!` on parse failure
     (matches existing pattern).
  3. Query ClickHouse for `MAX(time_unix_nano)` of spans matching `tenant_id` + `service_name`.
  4. Compute elapsed seconds since that timestamp (treat "no span ever seen" as infinitely
     stale — fires immediately, since acceptance criteria says "no span received in
     `window_secs`" with no carve-out for never-seen services).
  5. Fire (`record_firing`) if elapsed `>= window_secs`, else resolve (`resolve_open_firing`) —
     reuses the existing firing-state machine unchanged.
- Wire into `eval_alert_rules`: `eval_threshold_rules`, `eval_slo_burn_rate_rules`,
  `eval_composite_rules`, then `eval_deadman_rules`.
- Pure-logic unit tests (no DB/ClickHouse) for the elapsed-vs-window comparison, mirroring the
  existing `evaluate_threshold` test style: never-seen → fire, stale → fire, fresh → resolve,
  exactly-at-boundary → fire (consistent with `>=` matching `Gte` semantics elsewhere).

### API (`services/query-api/src/alerts.rs`)

- `CreateRuleRequest` gains:
  - `alert_type: Option<String>` (default `"threshold"` when omitted, for backward
    compatibility with existing callers/tests)
  - `service_name: Option<String>`
  - `window_secs: Option<i64>`
- `create_alert_rule`: branch on `alert_type`:
  - `"threshold"` (or omitted): existing behavior, unchanged.
  - `"deadman"`: validate `service_name` non-empty and `window_secs > 0` (return
    `CreateRuleError::InvalidInput` otherwise); build
    `condition = json!({"service_name": ..., "window_secs": ...})`; insert with
    `alert_type = 'deadman'`.
  - any other value: `CreateRuleError::InvalidInput`.
- `list_alert_rules`: query filter changes from `alert_type = 'threshold'` to
  `alert_type IN ('threshold', 'deadman')`.
- `condition_fields`: try the threshold shape (`metric_name`/`operator`/`threshold`) first; if
  any field is missing, try the deadman shape (`service_name`/`window_secs`) and map to
  `(service_name, "no_data", window_secs as f64)`. Rows that match neither shape are still
  skipped with the existing warning.
- `AlertRuleItem` struct/type is unchanged.

### Frontend (`apps/frontend/src/api/alerts.ts`, `apps/frontend/src/features/alerts/AlertsPage.tsx`)

- `CreateRuleRequest` (frontend type) gains the same three optional fields as the backend.
- Create-rule form: new "Alert type" `Select` with options "Threshold metric" (`threshold`) and
  "No data" (`deadman`), defaulting to `threshold`.
  - When `threshold` is selected: existing Metric name / Operator / Threshold value fields
    (unchanged).
  - When `deadman` is selected: those three fields are replaced by "Service name" (text input)
    and "Window (seconds)" (number input). Notification channels, auto-trigger-incident, and
    runbook URL fields are unchanged and apply to both types.
- `handleCreateSubmit`: branches on the selected alert type to build the right
  `CreateRuleRequest` payload and validates the deadman-specific fields (service name required,
  window a positive number) the same way the threshold path validates today.
- `AlertRuleRow` / `conditionLabel`: when `rule.operator === "no_data"`, render
  `"No data for {threshold}s from {metric_name}"` instead of `"{operator} {threshold}"`. Table
  column headers ("Metric", "Condition") stay generic enough to host both shapes without
  renaming.

## Testing plan

- Rust: new unit tests in `evaluator.rs` for `eval_deadman_rules`'s pure comparison logic, plus
  updates to any existing `alerts.rs` tests that assert on `list_alert_rules` filtering or
  `condition_fields` parsing. `cargo test -p alert-evaluator -p query-api`.
- Frontend: extend `AlertsPage` tests to cover selecting "No data", filling service
  name/window, and submitting; assert the resulting request payload and the rendered condition
  label for a deadman row. `npm test`.
- No new Testcontainers integration test: this follows the same DB/ClickHouse access pattern
  already covered by the threshold and SLO burn-rate eval paths.

## Files affected

- `services/alert-evaluator/src/evaluator.rs`
- `services/query-api/src/alerts.rs`
- `apps/frontend/src/api/alerts.ts`
- `apps/frontend/src/features/alerts/AlertsPage.tsx` (+ its test file)
- `docs/agent-context.md` (short note on completion, per repo convention)
- `docs/superpowers/plans/2026-06-04-observability-feature-parity-plan.md` (mark P12-S3 done)
