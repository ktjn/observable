# P5-S5: Composite Alert Evaluation Implementation Plan

> **Status:** Completed 2026-05-22

**Goal:** Evaluate one composite alert rule pair end-to-end so the alert evaluator can derive a firing from two already-active source alerts and resolve it when either source alert clears.

**Architecture:** Extend `services/alert-evaluator` with a composite-rule evaluator that reads a two-rule `condition` payload, checks the current active state of both referenced alert rules, and reuses the existing firing/incident lifecycle helpers. No schema migration is required.

**Scope:** Backend evaluator only. The slice does not add a composite-rule authoring UI or a new query API route. It proves the backend rule-pair semantics and keeps the existing threshold/SLO paths unchanged.

**Condition shape:** `alert_type = 'composite'` with `condition.rule_ids = [rule_id_a, rule_id_b]`. The composite rule fires only when both referenced rules are `active`.

---

## File Map

| File | Action | Purpose |
|------|--------|---------|
| `services/alert-evaluator/src/evaluator.rs` | Modify | Add composite rule parsing/evaluation and helper queries |
| `services/alert-evaluator/tests/lifecycle_integration.rs` | Modify | Add the failing/covering Testcontainers integration test |
| `spec/07-alerting-slo.md` | Modify | Document the rule-pair composite alert semantics |
| `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Modify | Mark P5-S5 complete |
| `docs/agent-context.md` | Modify | Record the completed composite-alert evaluator note |

---

## Tasks

- [x] Add a Testcontainers integration test that seeds two threshold rules and one composite rule, then proves the composite rule stays resolved while only one source rule is active, becomes active after both source rules fire, and resolves again when one source rule clears.
- [x] Add a composite condition struct, source-rule active-state helper, and composite evaluation pass after threshold and SLO evaluation.
- [x] Document the composite rule-pair semantics in the alerting spec.
- [x] Mark P5-S5 complete in the active roadmap and record the implementation in agent context.

---

## Verification

- `cargo fmt --all`
- `cargo test -p alert-evaluator --test lifecycle_integration composite_rule_tracks_two_source_rules -- --nocapture`
- `cargo test -p alert-evaluator --tests`
- `bash scripts/local-ci.sh --skip-frontend --skip-helm`

**Baseline:** composite rules were previously ignored by the evaluator.

**New errors introduced:** none.

**Telemetry impact:** none beyond the existing alert firing and incident lifecycle events.

**Auth/tenancy impact:** none; the evaluator remains tenant-scoped through existing rule and firing queries.

**Data retention or migration impact:** none; no schema or retention changes are required.

**Rollback path:** revert the evaluator change and the new test. No migration rollback is needed.

**ADR/spec sync:** spec update completed; no new ADR is required because this slice uses the existing alert-rule model and only defines evaluator semantics for an already-enumerated alert type.

**Checkpoint question:** can the platform derive one composite alert from two source rules without changing the existing alert lifecycle helpers? Answer: yes.

**Next smallest slice:** add composite-rule authoring to the alert-rule API and UI if operators need to create these rules without direct database writes.
