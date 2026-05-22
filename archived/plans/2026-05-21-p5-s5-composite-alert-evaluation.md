# Composite Alert Evaluation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one composite alert rule that fires when two existing alert rules are both active.

**Architecture:** Extend the existing alert-evaluator loop with one new `alert_type = 'composite'` pass that reads a pair of source rule IDs from `alert_rules.condition`, checks whether both source rules currently have active firings, and then reuses the existing firing/resolve machinery. Keep the first slice deliberately narrow: no new UI, no new incident model, and no new evaluator architecture beyond the composite pass.

**Tech Stack:** Rust, SQLx, ClickHouse, PostgreSQL, Testcontainers

---

### Task 1: Add the failing composite-evaluator integration test

**Files:**
- Modify: `services/alert-evaluator/tests/lifecycle_integration.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn composite_rule_fires_only_when_both_source_rules_are_active() {
    let (pool, _pg) = start_postgres().await;
    let (ch, _ch) = start_clickhouse().await;
    let tenant_id = Uuid::new_v4();
    let left_rule_id = create_threshold_rule(&pool, tenant_id, "left_metric", 0.05, None).await;
    let right_rule_id = create_threshold_rule(&pool, tenant_id, "right_metric", 0.10, None).await;
    let composite_rule_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO alert_rules \
         (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'left AND right', 'composite', 'critical', $3)",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .bind(serde_json::json!({
        "left_rule_id": left_rule_id,
        "right_rule_id": right_rule_id,
    }))
    .execute(&pool)
    .await
    .expect("composite rule inserted");

    insert_metric_point(&ch, tenant_id, "left_metric", 0.10).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();
    eval_alert_rules(&pool, &ch).await.unwrap();

    let first_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(first_count, 0);

    insert_metric_point(&ch, tenant_id, "right_metric", 0.20).await;
    eval_threshold_rules(&pool, &ch).await.unwrap();
    eval_alert_rules(&pool, &ch).await.unwrap();

    let second_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM alert_firings \
         WHERE rule_id = $1 AND tenant_id = $2 AND state = 'active'",
    )
    .bind(composite_rule_id)
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(second_count, 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p alert-evaluator composite_rule_fires_only_when_both_source_rules_are_active --test lifecycle_integration -- --nocapture`

Expected: FAIL because `eval_alert_rules` does not yet evaluate composite alert rules.

- [ ] **Step 3: Commit the red test**

```bash
git add services/alert-evaluator/tests/lifecycle_integration.rs
git commit -m "test(alert-evaluator): cover composite rule pair"
```

### Task 2: Implement composite alert evaluation

**Files:**
- Modify: `services/alert-evaluator/src/evaluator.rs`
- Modify: `services/alert-evaluator/src/lib.rs` if needed for exports

- [ ] **Step 1: Write the minimal implementation**

```rust
#[derive(Debug, Deserialize, Clone)]
pub struct CompositeRuleCondition {
    pub left_rule_id: Uuid,
    pub right_rule_id: Uuid,
}

pub async fn eval_composite_rules(
    db: &sqlx::PgPool,
    _ch: &clickhouse::Client,
) -> anyhow::Result<()> {
    let rules: Vec<AlertRuleRow> = sqlx::query_as(
        "SELECT rule_id, tenant_id, name, condition, severity, for_duration_secs, notification_channels, \
         auto_trigger_incident, auto_trigger_delay_secs, runbook_url \
         FROM alert_rules WHERE alert_type = 'composite' AND silenced = false",
    )
    .fetch_all(db)
    .await?;

    for rule in rules {
        let cond: CompositeRuleCondition = match serde_json::from_value(rule.condition.clone()) {
            Ok(cond) => cond,
            Err(_) => continue,
        };

        let left_active: Option<Uuid> = sqlx::query_scalar(
            "SELECT firing_id FROM alert_firings \
             WHERE tenant_id = $1 AND rule_id = $2 AND state = 'active' \
             LIMIT 1",
        )
        .bind(rule.tenant_id)
        .bind(cond.left_rule_id)
        .fetch_optional(db)
        .await?;

        let right_active: Option<Uuid> = sqlx::query_scalar(
            "SELECT firing_id FROM alert_firings \
             WHERE tenant_id = $1 AND rule_id = $2 AND state = 'active' \
             LIMIT 1",
        )
        .bind(rule.tenant_id)
        .bind(cond.right_rule_id)
        .fetch_optional(db)
        .await?;

        match (left_active, right_active) {
            (Some(_), Some(_)) => {
                if let Err(err) = record_firing(db, &rule, 1.0).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %err, "failed to record composite alert firing");
                }
            }
            _ => {
                if let Err(err) = resolve_open_firing(db, &rule, 0.0).await {
                    tracing::warn!(rule_id = %rule.rule_id, error = %err, "failed to resolve composite alert firing");
                }
            }
        }
    }

    Ok(())
}

pub async fn eval_alert_rules(db: &sqlx::PgPool, ch: &clickhouse::Client) -> anyhow::Result<()> {
    eval_threshold_rules(db, ch).await?;
    eval_slo_burn_rate_rules(db, ch).await?;
    eval_composite_rules(db, ch).await?;
    Ok(())
}
```

- [ ] **Step 2: Run the targeted test again**

Run: `cargo test -p alert-evaluator composite_rule_fires_only_when_both_source_rules_are_active --test lifecycle_integration -- --nocapture`

Expected: PASS, with one composite firing only after both source rules are active.

- [ ] **Step 3: Commit the implementation**

```bash
git add services/alert-evaluator/src/evaluator.rs services/alert-evaluator/src/lib.rs
git commit -m "feat(alert-evaluator): add composite rule pair evaluation"
```

### Task 3: Update roadmap and agent context

**Files:**
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`

- [ ] **Step 1: Mark P5-S5 complete in the roadmap**

```markdown
- [x] **P5-S5: Add composite alert evaluation for one rule pair** (COMPLETED 2026-05-21)
  - Direct prerequisite: threshold/SLO evaluator stability.
  - Completion signal: two existing signals can combine into one derived firing without duplicating evaluator architecture.
```

- [ ] **Step 2: Record the composite-condition convention in agent context**

```markdown
- `alert-evaluator` now supports a first composite alert slice: `alert_type = 'composite'` with `condition` fields `left_rule_id` and `right_rule_id`; the evaluator treats the pair as an `AND` and fires only when both source rules are active.
```

- [ ] **Step 3: Run the repo-local verification gate**

Run: `cargo fmt --all && bash scripts/local-ci.sh --skip-frontend --skip-helm`

Expected: PASS for the Rust and backend verification surfaces touched by this slice.

- [ ] **Step 4: Commit the docs update**

```bash
git add docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md docs/agent-context.md archived/plans/2026-05-21-p5-s5-composite-alert-evaluation.md
git commit -m "docs(alerting): close composite rule pair slice"
```

---

**Rollback path:** Remove the composite evaluator pass and the new condition parser; the existing threshold and SLO evaluation paths remain unchanged.

**Telemetry impact:** No new telemetry schema. Composite alerts reuse existing alert firings, notifications, and incident events.

**Auth/tenancy impact:** None beyond the existing tenant scoping already enforced in alert evaluation queries.

**Data retention or migration impact:** No migration required.

**ADR/spec sync:** No ADR update required. The composite pair is an implementation of the existing alerting/domain model and roadmap item; document the new condition shape in `docs/agent-context.md` only.

**Checkpoint question:** does the first composite alert slice stay read-only and evaluator-only, with no new UI or state machine behavior?

**Next smallest slice:** Add a second composite rule operator or expose composite-rule creation in the alert rule API/UI if product demand requires it.
