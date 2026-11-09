//! Rules, evaluation and scope-bound snapshots (phase 1 C2.3).
//!
//! The worker evaluates the current rule for every live service and environment class it can
//! read, and stores one snapshot per (tenant, service, class) keyed to the identities it used.
//! Disclosure recomputes freshness at read time, so an evaluator outage cannot keep an old
//! `normal` current.

use chrono::{DateTime, Duration, SubsecRound as _, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::ports::{
    CatalogInputs, EnvironmentClass, Evidence, EvidenceTimes, TelemetryIdentity, TenantInputs,
};
use crate::scope::{Scope, scope};

pub const SIGNALS: &[&str] = &["traces", "logs", "metrics", "any"];

/// One evidence condition of a rule: a signal that must be current, and the classification
/// of the evidence it yields.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    pub signal: String,
    pub classification: String,
}

/// A published rule revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rule {
    pub rule_id: String,
    pub revision: i32,
    pub freshness_seconds: i32,
    pub lookback_seconds: i32,
    pub conditions: Vec<Condition>,
}

/// The tenant's newest rule revision, else the newest platform default. A rule with a
/// condition this build cannot evaluate is treated as no rule.
pub async fn current_rule(db: &PgPool, tenant_id: Uuid) -> Result<Option<Rule>, sqlx::Error> {
    let row: Option<(String, i32, i32, i32, serde_json::Value)> = sqlx::query_as(
        "SELECT rule_id, revision, freshness_seconds, lookback_seconds, conditions \
         FROM health.rules WHERE tenant_id = $1 OR tenant_id IS NULL \
         ORDER BY (tenant_id IS NULL), revision DESC, published_at DESC LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(db)
    .await?;
    let Some((rule_id, revision, freshness_seconds, lookback_seconds, conditions)) = row else {
        return Ok(None);
    };
    let Ok(conditions) = serde_json::from_value::<Vec<Condition>>(conditions) else {
        return Ok(None);
    };
    let valid = !conditions.is_empty()
        && conditions.iter().all(|c| {
            SIGNALS.contains(&c.signal.as_str())
                && observable_auth::authz::Classification::parse(&c.classification).is_some()
        });
    Ok(valid.then_some(Rule {
        rule_id,
        revision,
        freshness_seconds,
        lookback_seconds,
        conditions,
    }))
}

/// Canonical key of the identities a snapshot was evaluated over.
pub fn scope_key(identities: &[TelemetryIdentity]) -> String {
    let mut parts: Vec<String> = identities
        .iter()
        .map(|i| format!("{}\u{1f}{}", i.service_name, i.environment))
        .collect();
    parts.sort();
    parts.join("\u{1e}")
}

/// Per-condition evidence time stored in a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionEvidence {
    pub signal: String,
    pub classification: String,
    pub last_evidence_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub tenant_id: Uuid,
    pub service_id: Uuid,
    pub environment_class: EnvironmentClass,
    pub rule_id: String,
    pub rule_revision: i32,
    pub scope_key: String,
    pub identities: Vec<TelemetryIdentity>,
    pub conditions: Vec<ConditionEvidence>,
    pub freshness_seconds: i32,
    pub evaluated_at: DateTime<Utc>,
}

/// Evidence status and state of a snapshot's conditions at `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Freshness {
    Current,
    Stale,
    Missing,
}

impl Freshness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Missing => "missing",
        }
    }
}

fn condition_freshness(
    c: &ConditionEvidence,
    freshness_seconds: i32,
    now: DateTime<Utc>,
) -> Freshness {
    match c.last_evidence_at {
        None => Freshness::Missing,
        Some(t) if now - t <= Duration::seconds(i64::from(freshness_seconds)) => Freshness::Current,
        Some(_) => Freshness::Stale,
    }
}

impl Snapshot {
    /// `normal` only when every condition has current evidence; otherwise `unknown`, with
    /// `stale` when some evidence exists and `missing` when none does.
    pub fn freshness(&self, now: DateTime<Utc>) -> (&'static str, Freshness) {
        let states: Vec<Freshness> = self
            .conditions
            .iter()
            .map(|c| condition_freshness(c, self.freshness_seconds, now))
            .collect();
        if !states.is_empty() && states.iter().all(|s| *s == Freshness::Current) {
            ("normal", Freshness::Current)
        } else if self.conditions.iter().any(|c| c.last_evidence_at.is_some()) {
            ("unknown", Freshness::Stale)
        } else {
            ("unknown", Freshness::Missing)
        }
    }

    pub fn last_evidence_at(&self) -> Option<DateTime<Utc>> {
        self.conditions
            .iter()
            .filter_map(|c| c.last_evidence_at)
            .max()
    }

    pub fn condition_states(&self, now: DateTime<Utc>) -> Vec<serde_json::Value> {
        self.conditions
            .iter()
            .map(|c| {
                json!({
                    "signal": c.signal,
                    "last_evidence_at": c.last_evidence_at,
                    "evidence_status": condition_freshness(c, self.freshness_seconds, now).as_str(),
                })
            })
            .collect()
    }
}

fn ns_to_time(ns: u64) -> Option<DateTime<Utc>> {
    i64::try_from(ns).ok().map(DateTime::from_timestamp_nanos)
}

fn signal_time(t: &EvidenceTimes, signal: &str) -> Option<u64> {
    match signal {
        "traces" => t.traces,
        "logs" => t.logs,
        "metrics" => t.metrics,
        _ => t.traces.max(t.logs).max(t.metrics),
    }
}

/// Evaluates one service in one class. `None` when evidence could not be read: nothing is
/// stored, and the previous snapshot goes stale at read time.
pub async fn evaluate(
    evidence: &Evidence,
    tenant_id: Uuid,
    service_id: Uuid,
    class: EnvironmentClass,
    identities: Vec<TelemetryIdentity>,
    rule: &Rule,
    started_at: DateTime<Utc>,
) -> Option<Snapshot> {
    // Stored with PostgreSQL's microsecond precision, so ordering compares like with like.
    let started_at = started_at.trunc_subsecs(6);
    let since = started_at - Duration::seconds(i64::from(rule.lookback_seconds));
    let since_ns = u64::try_from(since.timestamp_nanos_opt()?).ok()?;
    let mut latest = Vec::with_capacity(identities.len());
    for identity in &identities {
        latest.push(
            evidence
                .latest_since(tenant_id, identity, since_ns)
                .await
                .ok()?,
        );
    }
    let conditions = rule
        .conditions
        .iter()
        .map(|c| ConditionEvidence {
            signal: c.signal.clone(),
            classification: c.classification.clone(),
            last_evidence_at: latest
                .iter()
                .filter_map(|t| signal_time(t, &c.signal))
                .max()
                .and_then(ns_to_time),
        })
        .collect();
    Some(Snapshot {
        tenant_id,
        service_id,
        environment_class: class,
        rule_id: rule.rule_id.clone(),
        rule_revision: rule.revision,
        scope_key: scope_key(&identities),
        identities,
        conditions,
        freshness_seconds: rule.freshness_seconds,
        evaluated_at: started_at,
    })
}

/// Stores a snapshot unless a newer evaluation already did. Returns whether it was stored.
pub async fn persist(db: &PgPool, s: &Snapshot) -> Result<bool, sqlx::Error> {
    let (state, _) = s.freshness(s.evaluated_at);
    let identities: Vec<serde_json::Value> = s
        .identities
        .iter()
        .map(|i| json!({"service_name": i.service_name, "environment": i.environment}))
        .collect();
    let stored = sqlx::query(
        "INSERT INTO health.assessments (tenant_id, service_id, environment_class, rule_id, \
         rule_revision, scope_key, identities, state, conditions, freshness_seconds, \
         last_evidence_at, evaluated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (tenant_id, service_id, environment_class) DO UPDATE SET \
         rule_id = EXCLUDED.rule_id, rule_revision = EXCLUDED.rule_revision, \
         scope_key = EXCLUDED.scope_key, identities = EXCLUDED.identities, state = EXCLUDED.state, \
         conditions = EXCLUDED.conditions, freshness_seconds = EXCLUDED.freshness_seconds, \
         last_evidence_at = EXCLUDED.last_evidence_at, evaluated_at = EXCLUDED.evaluated_at \
         WHERE health.assessments.evaluated_at < EXCLUDED.evaluated_at",
    )
    .bind(s.tenant_id)
    .bind(s.service_id)
    .bind(s.environment_class.as_str())
    .bind(&s.rule_id)
    .bind(s.rule_revision)
    .bind(&s.scope_key)
    .bind(json!(identities))
    .bind(state)
    .bind(json!(s.conditions))
    .bind(s.freshness_seconds)
    .bind(s.last_evidence_at())
    .bind(s.evaluated_at)
    .execute(db)
    .await?
    .rows_affected();
    Ok(stored == 1)
}

type SnapshotRow = (
    String,
    i32,
    String,
    serde_json::Value,
    serde_json::Value,
    i32,
    DateTime<Utc>,
);

pub async fn load(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
    class: EnvironmentClass,
) -> Result<Option<Snapshot>, sqlx::Error> {
    let row: Option<SnapshotRow> = sqlx::query_as(
        "SELECT rule_id, rule_revision, scope_key, identities, conditions, freshness_seconds, \
         evaluated_at FROM health.assessments \
         WHERE tenant_id = $1 AND service_id = $2 AND environment_class = $3",
    )
    .bind(tenant_id)
    .bind(service_id)
    .bind(class.as_str())
    .fetch_optional(db)
    .await?;
    let Some((rule_id, rule_revision, scope_key, identities, conditions, freshness, at)) = row
    else {
        return Ok(None);
    };
    #[derive(Deserialize)]
    struct Identity {
        service_name: String,
        environment: String,
    }
    let identities: Vec<Identity> = serde_json::from_value(identities).unwrap_or_default();
    let Ok(conditions) = serde_json::from_value::<Vec<ConditionEvidence>>(conditions) else {
        return Ok(None);
    };
    Ok(Some(Snapshot {
        tenant_id,
        service_id,
        environment_class: class,
        rule_id,
        rule_revision,
        scope_key,
        identities: identities
            .into_iter()
            .map(|i| TelemetryIdentity {
                service_name: i.service_name,
                environment: i.environment,
            })
            .collect(),
        conditions,
        freshness_seconds: freshness,
        evaluated_at: at,
    }))
}

/// Outcome counts of one evaluation pass over a tenant.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassSummary {
    pub stored: usize,
    pub superseded: usize,
    pub failed: usize,
}

/// Evaluates every live service of a tenant in both classes from one catalog read.
pub async fn evaluate_inputs(
    db: &PgPool,
    evidence: &Evidence,
    inputs: &TenantInputs,
    rule: &Rule,
    started_at: DateTime<Utc>,
) -> Result<PassSummary, sqlx::Error> {
    let mut summary = PassSummary::default();
    for service in &inputs.services {
        for class in [
            EnvironmentClass::Production,
            EnvironmentClass::NonProduction,
        ] {
            match scope(inputs, service.id, class) {
                Scope::Bound(identities) => {
                    match evaluate(
                        evidence,
                        inputs.tenant_id,
                        service.id,
                        class,
                        identities,
                        rule,
                        started_at,
                    )
                    .await
                    {
                        Some(snapshot) => {
                            if persist(db, &snapshot).await? {
                                summary.stored += 1;
                            } else {
                                summary.superseded += 1;
                            }
                        }
                        None => summary.failed += 1,
                    }
                }
                Scope::NotEvaluated => {}
                Scope::Unavailable => summary.failed += 1,
            }
        }
    }
    Ok(summary)
}

/// One pass over every tenant the worker holds a credential for. A tenant whose inputs or
/// rule cannot be read is skipped; its snapshots go stale at read time.
pub async fn evaluate_all(
    db: &PgPool,
    catalog: &CatalogInputs,
    evidence: &Evidence,
) -> PassSummary {
    let mut total = PassSummary::default();
    for tenant_id in catalog.tenants() {
        let started_at = Utc::now();
        let rule = match current_rule(db, tenant_id).await {
            Ok(Some(rule)) => rule,
            Ok(None) | Err(_) => {
                total.failed += 1;
                tracing::warn!(%tenant_id, "health rule unavailable");
                continue;
            }
        };
        let Ok(inputs) = catalog.inputs(tenant_id).await else {
            total.failed += 1;
            tracing::warn!(%tenant_id, "health catalog inputs unavailable");
            continue;
        };
        match evaluate_inputs(db, evidence, &inputs, &rule, started_at).await {
            Ok(s) => {
                total.stored += s.stored;
                total.superseded += s.superseded;
                total.failed += s.failed;
            }
            Err(e) => {
                total.failed += 1;
                tracing::warn!(%tenant_id, error = %e, "health snapshot store unavailable");
            }
        }
    }
    total
}

pub async fn start_evaluation_worker(
    db: PgPool,
    catalog: CatalogInputs,
    evidence: Evidence,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        let s = evaluate_all(&db, &catalog, &evidence).await;
        tracing::debug!(
            stored = s.stored,
            superseded = s.superseded,
            failed = s.failed,
            "health evaluation pass"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(conditions: Vec<Option<i64>>, now: DateTime<Utc>) -> Snapshot {
        Snapshot {
            tenant_id: Uuid::nil(),
            service_id: Uuid::nil(),
            environment_class: EnvironmentClass::Production,
            rule_id: "r".into(),
            rule_revision: 1,
            scope_key: String::new(),
            identities: vec![],
            conditions: conditions
                .into_iter()
                .map(|age| ConditionEvidence {
                    signal: "any".into(),
                    classification: "internal".into(),
                    last_evidence_at: age.map(|s| now - Duration::seconds(s)),
                })
                .collect(),
            freshness_seconds: 300,
            evaluated_at: now,
        }
    }

    #[test]
    fn normal_needs_every_condition_current() {
        let now = Utc::now();
        assert_eq!(
            snapshot(vec![Some(10), Some(299)], now).freshness(now),
            ("normal", Freshness::Current)
        );
        assert_eq!(
            snapshot(vec![Some(10), Some(301)], now).freshness(now),
            ("unknown", Freshness::Stale)
        );
        assert_eq!(
            snapshot(vec![Some(10), None], now).freshness(now),
            ("unknown", Freshness::Stale)
        );
        assert_eq!(
            snapshot(vec![None], now).freshness(now),
            ("unknown", Freshness::Missing)
        );
        assert_eq!(
            snapshot(vec![], now).freshness(now),
            ("unknown", Freshness::Missing)
        );
    }

    #[test]
    fn freshness_is_evaluated_at_read_time() {
        let then = Utc::now() - Duration::minutes(30);
        let s = snapshot(vec![Some(5)], then);
        assert_eq!(s.freshness(then).0, "normal");
        assert_eq!(s.freshness(Utc::now()), ("unknown", Freshness::Stale));
    }

    #[test]
    fn scope_key_is_order_independent() {
        let a = TelemetryIdentity {
            service_name: "a".into(),
            environment: "prod".into(),
        };
        let b = TelemetryIdentity {
            service_name: "b".into(),
            environment: "prod".into(),
        };
        assert_eq!(
            scope_key(&[a.clone(), b.clone()]),
            scope_key(&[b.clone(), a.clone()])
        );
        assert_ne!(scope_key(std::slice::from_ref(&a)), scope_key(&[a, b]));
        assert_eq!(scope_key(&[]), "");
    }
}
