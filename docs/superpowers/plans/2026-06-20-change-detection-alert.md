# Change-Detection Alert Type Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a third alert rule type, `change_detection`, that compares a current window's metric
average against a baseline window N hours/days back and fires when the percent change exceeds a
configurable threshold. Roadmap: Tier 1, "Change-Detection Alert Type" (was P12-S4) in
`docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md`.

**Architecture:** This repeats the established pattern for adding a new alert type, set most
recently by the deadman alert (`archived/plans/2026-06-18-p12-s3-deadman-alert.md`, fully shipped).
`alert_rules.alert_type` already has a Postgres CHECK constraint listing `change_detection` as a
valid value (`migrations/postgres/007_create_alert_rules.sql`) — **no migration needed**. The work
is: a pure `evaluate_change_detection` function in `alert-evaluator`, an `eval_change_detection_rules`
loop that fetches two ClickHouse window averages and calls it, `query-api` CRUD support
(`condition_fields`, `create_alert_rule`, `list_alert_rules`'s type filter), and a third branch in
the frontend's alert-type selector.

**Tech Stack:** Rust (clickhouse, sqlx) in `services/alert-evaluator` and `services/query-api`;
React 19 in `apps/frontend/src/features/alerts/AlertsPage.tsx`.

## Global Constraints

- No Postgres migration — `change_detection` is already a valid `alert_type` enum value.
- Condition JSON shape — matches the original P12-S4 acceptance criteria in
  `archived/plans/2026-06-04-observability-feature-parity-plan.md:659-669` exactly (`metric`,
  `window_secs`, `baseline_offset_secs`, `threshold_percent` — one `window_secs` sizes both the
  current and baseline windows, there is no separate baseline window size):
  ```json
  {
    "metric_name": "error_rate",
    "window_secs": 300,
    "baseline_offset_secs": 86400,
    "threshold_percent": 50.0
  }
  ```
  `baseline_offset_secs` is how far back the baseline window *starts* (e.g. 86400 = "compare against
  the same time yesterday"). The baseline window spans
  `[now - baseline_offset_secs - window_secs, now - baseline_offset_secs]`; the current window spans
  `[now - window_secs, now]`.
- Percent change formula: `((current_avg - baseline_avg) / baseline_avg) * 100`, using absolute
  value against `threshold_percent` (fires on either a spike or a drop) — the user story says
  "changes by more than X%", not "increases by", so bidirectional is correct per the source spec,
  not just a fallback default.
- Guard divide-by-zero: if `baseline_avg == 0.0`, treat any non-zero current value as firing (100%+
  change), matching the existing `EvalResult` two-state model — do not introduce a third "unknown"
  state.
- `cargo fmt --all` after every Rust edit, before staging.
- Add a Testcontainers/ClickHouse integration test for the new evaluator path (roadmap §1 rule 5) —
  follow whatever pattern `services/alert-evaluator/tests/` already uses for SLO burn-rate or
  deadman (check for `slo_burn_rate_integration.rs` / equivalent deadman test file and mirror its
  structure, including ClickHouse container setup and metric-point seeding).

## Tasks

- [ ] **1. Design note + pure evaluation logic**
  - Write a short design note at
    `docs/superpowers/specs/2026-06-20-change-detection-alert-design.md` (mirroring
    `docs/superpowers/specs/2026-06-18-p12-s3-deadman-alert-design.md`'s structure) fixing the
    condition JSON shape above, the percent-change formula and directionality decision, and the
    divide-by-zero rule.
  - `services/alert-evaluator/src/evaluator.rs`: add `ChangeDetectionCondition` struct (alongside
    `DeadmanCondition`, ~line 56) with fields `metric_name: String`, `window_secs: i64`,
    `baseline_offset_secs: i64`, `threshold_percent: f64`.
  - Add `pub fn evaluate_change_detection(current_avg: f64, baseline_avg: f64, condition: &ChangeDetectionCondition) -> EvalResult`
    — pure function, no I/O, implementing the formula and zero-guard from Global Constraints.
  - Unit tests in the same file (`#[cfg(test)] mod tests`, matching the existing
    `evaluate_threshold`/`evaluate_deadman` test style) covering: spike fires, drop fires
    (bidirectional), within-threshold doesn't fire, baseline-zero-with-nonzero-current fires,
    baseline-zero-with-zero-current doesn't fire.

- [ ] **2. `alert-evaluator`: ClickHouse window-average fetch + evaluation loop**
  - Add a `WindowAvgRow` struct (`#[derive(clickhouse::Row, serde::Deserialize)]`, alongside
    `LatestPointRow`/`LastSeenRow`) with a single `avg_value: f64` field (or `Option<f64>` if
    ClickHouse returns NULL for an empty window — confirm and handle the `None`-means-skip-rule
    case explicitly, do not coerce to 0.0 silently).
  - Add `eval_change_detection_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()>`,
    placed after `eval_deadman_rules` (~line 543), following that function's exact structural
    pattern: `SELECT ... FROM alert_rules WHERE alert_type = 'change_detection' AND silenced = false`,
    parse condition JSON (skip + warn on parse failure), then for each rule run two ClickHouse
    queries against `observable.metric_points`:
    ```sql
    SELECT avg(value_double) AS avg_value FROM observable.metric_points
    WHERE tenant_id = ? AND metric_name = ?
      AND time_unix_nano BETWEEN ? AND ?
    ```
    bound once for the current window (`now - window_secs` to `now`) and once for the
    baseline window (`now - baseline_offset_secs - window_secs` to
    `now - baseline_offset_secs`). Convert seconds to nanosecond unix timestamps the same way the
    existing deadman/threshold queries do (check what unit `time_unix_nano` columns expect, mirror
    existing bind conversions rather than inventing a new one).
  - On `None`/missing average for either window, skip the rule with `tracing::warn!` (same pattern
    as threshold's "skipping rule: metric fetch failed").
  - Call `evaluate_change_detection`, then `record_firing`/`resolve_open_firing` exactly as the other
    `eval_*_rules` functions do, logging the firing case with `tracing::warn!` including
    `current_avg`, `baseline_avg`, `threshold_percent`.
  - Wire into `eval_alert_rules` (line 544-550): add `eval_change_detection_rules(db, ch).await?;`
    after the `eval_deadman_rules` call.
  - Test: extend or add `services/alert-evaluator/tests/<existing-pattern>.rs` with a Testcontainers
    case seeding metric points in two distinct time ranges and asserting a firing transitions
    `alert_firings` to `active`, plus a within-threshold case that stays `ok`.

- [ ] **3. `query-api`: CRUD support**
  - `services/query-api/src/alerts.rs`:
    - `condition_fields()` (line 153-168): add a third shape-matching branch for change-detection
      condition JSON (has `metric_name` + `threshold_percent` but no `operator`/`threshold` —
      disambiguate by checking for `threshold_percent` specifically before falling through to the
      deadman branch, since both lack `operator`). Map it to the existing
      `(metric_name, operator, threshold)` tuple as `(metric_name, "change_detection", threshold_percent)`
      so the existing `AlertRuleItem` shape needs no change (same approach as deadman, which reused
      `"no_data"` as a synthetic operator string).
    - `CreateRuleRequest` (line 91-103): add `window_secs_cd: Option<i64>` if `window_secs` would
      collide with the existing deadman `window_secs` field name — check: `CreateRuleRequest`
      already has `window_secs: Option<i64>` for deadman (line 102), and change-detection's window
      is semantically the same shape (a positive duration in seconds), so **reuse the existing
      `window_secs` field** rather than adding a near-duplicate; add only
      `baseline_offset_secs: Option<i64>` and `threshold_percent: Option<f64>` as new fields.
    - `create_alert_rule` (line 230-357): add a `"change_detection" =>` match arm following the
      `"deadman"` arm's structure — validate `metric_name` non-empty, all four numeric fields
      present and positive (`threshold_percent` must be finite and non-negative), build the
      condition JSON per the design note's shape, insert with `alert_type = 'change_detection'`,
      return an `AlertRuleItem` with `metric_name`, `operator: "change_detection"`,
      `threshold: threshold_percent`.
    - `list_alert_rules`'s `WHERE r.alert_type IN ('threshold', 'deadman')` (line 196): add
      `'change_detection'`.
  - Test: extend the existing Postgres Testcontainers test file covering alert-rule CRUD with a
    create/list round-trip for `change_detection`, plus an invalid-input case (e.g. missing
    `threshold_percent`).

- [ ] **4. Frontend: alert-type selector and form fields**
  - `apps/frontend/src/api/alerts.ts`: widen the `CreateRuleRequest` TS interface with
    `baseline_offset_secs?: number` and `threshold_percent?: number` (reuse the existing
    `window_secs` field, shared with deadman); widen any `alert_type` union type to include
    `"change_detection"`.
  - `apps/frontend/src/features/alerts/AlertsPage.tsx`:
    - Widen `formAlertType`'s type (line 41) to `"threshold" | "deadman" | "change_detection"` and
      add state vars for `formMetric` (reuse the existing threshold metric-name state if shared),
      `formBaselineOffsetSecs`, `formThresholdPercent` (mirror `formServiceName`/
      `formWindowSecs`'s declaration pattern; reuse `formWindowSecs` for the change-detection
      window since the condition shape shares that field with deadman).
    - Add a third `<SelectOption value="change_detection">` to the alert-type `<Select>` (line
      254-261), label e.g. "Change detection".
    - Extend the `formAlertType === "threshold" ? (...) : (...)` ternary (line 275) into a 3-way
      branch (or a `switch`/lookup) adding a metric-name field plus three numeric inputs (window
      secs, baseline offset secs, threshold percent) for the change-detection case, following the
      existing deadman block's grid/label/Input structure (line 316-339) as the visual template.
    - Extend `handleCreateSubmit` (line 110-154) with a `change_detection` branch before the
      existing `deadman`-then-`threshold` checks, validating the numeric fields are positive finite
      numbers and calling `createMutation.mutate` with `alert_type: "change_detection"`,
      `metric_name`, `window_secs`, `baseline_offset_secs`, `threshold_percent`.
  - Add/extend MSW handlers for the widened create payload (roadmap §1 rule 4) — find the existing
    alerts MSW handler file and extend it to accept the new fields.
  - Extend `AlertsPage.test.tsx` (or equivalent) with a case selecting "Change detection", filling
    the form, and asserting the right payload shape is submitted — follow whatever pattern the
    existing deadman-creation test in that file already uses.

- [ ] **5. Spec sync**
  - `spec/07-alerting-slo.md` §11.1: change detection is already listed as a first-class type — add
    an implementation note analogous to the deadman entry (condition shape, percent-change formula,
    directionality, zero-baseline rule) so the spec matches what's built.

- [ ] **6. Roadmap closeout**
  - Check off "Change-Detection Alert Type" in
    `docs/superpowers/plans/2026-06-19-unified-feature-roadmap.md` §3.
  - Add a closure entry to `docs/agent-context.md` matching the existing closeout style.
  - Move this plan file to `archived/plans/`.

## Verification

- `cargo fmt --all --check` and `cargo test -p alert-evaluator -p query-api` (Testcontainers tests
  need Docker).
- Frontend: `npm run typecheck` and `npm run test` in `apps/frontend/`.
- Manual: create a change-detection rule with a low threshold against a metric with known
  historical values, confirm it fires when a fresh window's average diverges and resolves when it
  reconverges.

## Rollback

- Standard code revert — no migration, no destructive schema change, nothing to roll back at the
  data layer.
