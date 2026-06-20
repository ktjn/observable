# Change Detection Alert Type — Design

Status: In Progress
Date: 2026-06-20
Roadmap reference: `docs/superpowers/plans/2026-06-19-change-event-api-dashboard-overlay.md`

## Problem

Existing alert types (`threshold`, `slo_burn_rate`, `deadman`, `composite`) all evaluate either absolute metric values or absence of telemetry. None detect rapid changes in metric behavior relative to a baseline, which is useful for identifying anomalies that may not exceed a fixed threshold but represent a significant deviation from normal pattern (e.g., latency jumping 2x, error rate dropping by half).

## Goal

Add a `change_detection` alert rule type: fires when a metric's recent average changes by more than a configurable percentage from a baseline window average, using the same rule/firing/notification machinery as the existing alert types.

## Non-goals

- Per-environment or per-label scoping in the condition shape. The condition carries only `metric_name`, matching acceptance criteria. Advanced carving can be added later.
- Different percent-change calculations for increases vs. decreases. The alert is bidirectional ("changes by more than X%"), not directional ("increases by" or "decreases by").
- Post-hoc window-computation logic in the evaluator. The evaluator takes two pre-computed averages and compares them; the caller (a later task) fetches and averages the data from ClickHouse.

## Design

### Condition JSON shape

```json
{
  "metric_name": "error_rate",
  "window_secs": 300,
  "baseline_offset_secs": 86400,
  "threshold_percent": 50.0
}
```

- `metric_name`: name of the metric to evaluate (e.g., `"error_rate"`, `"latency_p99_ms"`).
- `window_secs`: duration in seconds of the *current* window over which to compute the current average.
- `baseline_offset_secs`: how many seconds *back* the baseline window *starts*. The baseline window spans `[now - baseline_offset_secs - window_secs, now - baseline_offset_secs]`; the current window spans `[now - window_secs, now]`. E.g., with `window_secs=300` and `baseline_offset_secs=86400`, the current window is the last 5 minutes, and the baseline is 5 minutes from 24+ hours ago.
- `threshold_percent`: absolute percent change (≥0), symmetric for both increase and decrease. Alert fires if `|((current_avg - baseline_avg) / baseline_avg) * 100| >= threshold_percent`.

### Percent-change formula and directionality

The evaluator uses the formula: `|((current_avg - baseline_avg) / baseline_avg) * 100| >= threshold_percent`.

- **Bidirectional**: the alert fires on either a spike or a drop. A 100→150 change (50% increase) and a 100→50 change (50% decrease) both fire with a 50% threshold.
- **Divide-by-zero handling**:
  - If `baseline_avg == 0` and `current_avg != 0`: treat as 100%+ change → fire (because any non-zero is infinitely different from zero).
  - If `baseline_avg == 0` and `current_avg == 0`: do not fire (no meaningful change).
  - If `baseline_avg != 0`: use the normal formula.

### Backend evaluator (`services/alert-evaluator/src/evaluator.rs`)

#### Pure-logic layer (this task)

- `ChangeDetectionCondition { metric_name: String, window_secs: i64, baseline_offset_secs: i64, threshold_percent: f64 }` — deserialized from `alert_rules.condition` JSONB, same pattern as `ThresholdCondition` / `DeadmanCondition`.
- `pub fn evaluate_change_detection(current_avg: f64, baseline_avg: f64, condition: &ChangeDetectionCondition) -> EvalResult` — a pure function (no I/O, no database, no ClickHouse) that:
  1. Computes the percent change using the formula above.
  2. Returns `EvalResult::Firing` if the change meets or exceeds `threshold_percent`, else `EvalResult::Ok`.
  3. Handles the divide-by-zero cases as specified.
- Unit tests (no DB/ClickHouse) covering:
  - Spike fires (e.g., 100→150 with 50% threshold → firing).
  - Drop fires (e.g., 100→50 with 50% threshold → firing, bidirectional).
  - Within-threshold doesn't fire (e.g., 100→110 with 50% threshold → ok).
  - Baseline zero with non-zero current fires (0→10 with 50% threshold → firing).
  - Baseline zero with zero current doesn't fire (0→0 with 50% threshold → ok).

#### Database-querying layer (future task)

A future task will add `eval_change_detection_rules` (analogous to `eval_deadman_rules`, `eval_threshold_rules`) that:
1. Fetches rules where `alert_type = 'change_detection' AND silenced = false`.
2. For each rule, parses `ChangeDetectionCondition`; skips on parse failure with tracing::warn.
3. Queries ClickHouse for the current and baseline window averages (two queries or one query with two aggregations).
4. Calls `evaluate_change_detection(current_avg, baseline_avg, &condition)`.
5. Records or resolves firings via the existing `record_firing` / `resolve_open_firing` functions.
6. Wires into `eval_alert_rules` after the existing evaluator functions.

## Testing plan

- **Rust (this task)**: new unit tests in `evaluator.rs` for `evaluate_change_detection`, covering all cases listed above. `cargo test -p alert-evaluator`.
- **Rust (future task)**: integration test for the full eval cycle (DB + ClickHouse) once the eval function is wired in.
- **Frontend**: form + table rendering will be added in a later task.

## Files affected

- `services/alert-evaluator/src/evaluator.rs` (struct, function, unit tests — this task)
- `services/alert-evaluator/src/evaluator.rs` (eval_change_detection_rules, eval_alert_rules wiring — future task)
- `services/query-api/src/alerts.rs` (CreateRuleRequest, create_alert_rule branching — future task)
- `apps/frontend/src/api/alerts.ts` and `apps/frontend/src/features/alerts/AlertsPage.tsx` (form + table — future task)
