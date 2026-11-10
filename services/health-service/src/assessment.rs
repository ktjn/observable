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

/// Shortest threshold window, and the most conditions a rule may hold.
pub const MIN_WINDOW_SECONDS: i32 = 60;
pub const MAX_CONDITIONS: usize = 16;
/// Longest lookback a published rule may use (30 days).
pub const MAX_LOOKBACK_SECONDS: i32 = 30 * 86_400;

/// What a condition measures.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionKind {
    /// The signal has evidence within the freshness bound.
    #[default]
    Presence,
    /// Share of failed server requests in the window.
    ErrorRatio,
    /// Server request duration quantile in the window, in milliseconds.
    LatencyQuantile,
}

impl ConditionKind {
    fn is_presence(&self) -> bool {
        *self == Self::Presence
    }
}

/// State a breached threshold condition yields. `Unavailable` ranks above `Degraded`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Yields {
    Degraded,
    Unavailable,
}

impl Yields {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Degraded => "degraded",
            Self::Unavailable => "unavailable",
        }
    }
}

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

/// One evidence condition of a rule. A `presence` condition needs current evidence of its
/// signal. A threshold condition (`error_ratio`, `latency_quantile`) measures server requests
/// over `window_seconds` and yields its state when breached; with fewer than `min_requests`
/// it has insufficient evidence, never a breach.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    #[serde(default, skip_serializing_if = "ConditionKind::is_presence")]
    pub kind: ConditionKind,
    pub signal: String,
    pub classification: String,
    /// `error_ratio`: failed share in (0, 1]. `latency_quantile`: milliseconds above which
    /// the quantile breaches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    /// `latency_quantile` only: quantile level in (0, 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quantile: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_seconds: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_requests: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yields: Option<Yields>,
    /// A required condition must be current (and, for thresholds, sufficient) for `normal`.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub required: bool,
}

impl Condition {
    pub fn presence(signal: &str, classification: &str) -> Self {
        Self {
            kind: ConditionKind::Presence,
            signal: signal.into(),
            classification: classification.into(),
            threshold: None,
            quantile: None,
            window_seconds: None,
            min_requests: None,
            yields: None,
            required: true,
        }
    }

    /// The state a breach yields: latency yields only `degraded`.
    pub fn effective_yields(&self) -> Option<Yields> {
        match self.kind {
            ConditionKind::Presence => None,
            ConditionKind::ErrorRatio => self.yields,
            ConditionKind::LatencyQuantile => Some(Yields::Degraded),
        }
    }

    fn validate(&self, lookback_seconds: i32) -> Result<(), String> {
        if !SIGNALS.contains(&self.signal.as_str()) {
            return Err(format!("unsupported signal {:?}", self.signal));
        }
        if observable_auth::authz::Classification::parse(&self.classification).is_none() {
            return Err(format!(
                "unsupported classification {:?}",
                self.classification
            ));
        }
        if self.kind == ConditionKind::Presence {
            if self.threshold.is_some()
                || self.quantile.is_some()
                || self.window_seconds.is_some()
                || self.min_requests.is_some()
                || self.yields.is_some()
            {
                return Err("presence takes only signal and classification".into());
            }
            if !self.required {
                return Err("presence conditions are always required".into());
            }
            return Ok(());
        }
        if self.signal != "traces" {
            return Err("threshold conditions measure server spans: signal must be traces".into());
        }
        let window = self.window_seconds.ok_or("window_seconds is required")?;
        if window < MIN_WINDOW_SECONDS || window > lookback_seconds {
            return Err(format!(
                "window_seconds must be between {MIN_WINDOW_SECONDS} and lookback_seconds"
            ));
        }
        if self.min_requests.is_none_or(|n| n == 0) {
            return Err("min_requests must be at least 1".into());
        }
        let threshold = self
            .threshold
            .filter(|t| t.is_finite())
            .ok_or("threshold is required")?;
        match self.kind {
            ConditionKind::ErrorRatio => {
                if self.quantile.is_some() {
                    return Err("error_ratio takes no quantile".into());
                }
                if !(threshold > 0.0 && threshold <= 1.0) {
                    return Err("error_ratio threshold must be in (0, 1]".into());
                }
                match self.yields {
                    None => return Err("yields is required".into()),
                    // Loss of service needs affirmative proof: every request failing.
                    Some(Yields::Unavailable) if threshold < 1.0 => {
                        return Err(
                            "unavailable requires threshold 1 (every request failing)".into()
                        );
                    }
                    Some(_) => {}
                }
            }
            ConditionKind::LatencyQuantile => {
                if !self.quantile.is_some_and(|q| q > 0.0 && q < 1.0) {
                    return Err("quantile must be in (0, 1)".into());
                }
                if threshold <= 0.0 {
                    return Err("latency threshold must be positive milliseconds".into());
                }
                if self.yields == Some(Yields::Unavailable) {
                    return Err("latency yields only degraded".into());
                }
            }
            ConditionKind::Presence => unreachable!(),
        }
        Ok(())
    }
}

/// A published rule revision.
#[derive(Clone, Debug, PartialEq)]
pub struct Rule {
    pub rule_id: String,
    pub revision: i32,
    pub freshness_seconds: i32,
    pub lookback_seconds: i32,
    pub conditions: Vec<Condition>,
}

/// Checks a rule's bounds and conditions. A rule needs at least one presence condition, so
/// `normal` always rests on current evidence.
pub fn validate_rule(
    freshness_seconds: i32,
    lookback_seconds: i32,
    conditions: &[Condition],
) -> Result<(), String> {
    if freshness_seconds <= 0 {
        return Err("freshness_seconds must be positive".into());
    }
    if lookback_seconds < freshness_seconds || lookback_seconds > MAX_LOOKBACK_SECONDS {
        return Err("lookback_seconds must be between freshness_seconds and 30 days".into());
    }
    if conditions.is_empty() || conditions.len() > MAX_CONDITIONS {
        return Err(format!("a rule has 1 to {MAX_CONDITIONS} conditions"));
    }
    if !conditions.iter().any(|c| c.kind == ConditionKind::Presence) {
        return Err("a rule needs at least one presence condition".into());
    }
    for (i, c) in conditions.iter().enumerate() {
        c.validate(lookback_seconds)
            .map_err(|e| format!("conditions[{i}]: {e}"))?;
    }
    Ok(())
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
    let valid = validate_rule(freshness_seconds, lookback_seconds, &conditions).is_ok();
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

/// Result of a threshold condition at evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Ok,
    Breached,
    /// Fewer requests than `min_requests`: no conclusion either way.
    Insufficient,
}

impl Outcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Breached => "breached",
            Self::Insufficient => "insufficient",
        }
    }
}

/// Per-condition evidence stored in a snapshot. For a threshold condition,
/// `last_evidence_at` is the latest server request in the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConditionEvidence {
    #[serde(default, skip_serializing_if = "ConditionKind::is_presence")]
    pub kind: ConditionKind,
    pub signal: String,
    pub classification: String,
    pub last_evidence_at: Option<DateTime<Utc>>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    /// Error ratio, or quantile in milliseconds; absent when insufficient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requests: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yields: Option<Yields>,
}

impl ConditionEvidence {
    pub fn presence(signal: &str, classification: &str, at: Option<DateTime<Utc>>) -> Self {
        Self {
            kind: ConditionKind::Presence,
            signal: signal.into(),
            classification: classification.into(),
            last_evidence_at: at,
            required: true,
            outcome: None,
            value: None,
            requests: None,
            yields: None,
        }
    }
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
    /// Current requests, but fewer than a required threshold condition needs.
    Insufficient,
}

impl Freshness {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Missing => "missing",
            Self::Insufficient => "insufficient",
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
        Some(t) if now - t <= Duration::seconds(i64::from(freshness_seconds)) => {
            if c.outcome == Some(Outcome::Insufficient) {
                Freshness::Insufficient
            } else {
                Freshness::Current
            }
        }
        Some(_) => Freshness::Stale,
    }
}

impl Snapshot {
    /// State and evidence status at `now`:
    /// - The worst breach among current threshold conditions wins (`unavailable` over
    ///   `degraded`). A breach that is no longer current has no effect.
    /// - Otherwise `normal` only when every required condition is current and sufficient.
    /// - Otherwise `unknown`: `stale` when required evidence aged out, `missing` when there is
    ///   no evidence at all, `insufficient` when a required threshold lacks requests, else
    ///   `stale`.
    ///
    /// Absence of data is never a breach, so missing data never yields `unavailable`.
    pub fn freshness(&self, now: DateTime<Utc>) -> (&'static str, Freshness) {
        let states: Vec<Freshness> = self
            .conditions
            .iter()
            .map(|c| condition_freshness(c, self.freshness_seconds, now))
            .collect();
        let required_current = self.conditions.iter().any(|c| c.required)
            && self
                .conditions
                .iter()
                .zip(&states)
                .all(|(c, s)| !c.required || *s == Freshness::Current);
        let required = || {
            self.conditions
                .iter()
                .zip(&states)
                .filter(|(c, _)| c.required)
        };
        let status = if required_current {
            Freshness::Current
        } else if required().any(|(_, s)| *s == Freshness::Stale) {
            Freshness::Stale
        } else if self.conditions.iter().all(|c| c.last_evidence_at.is_none()) {
            Freshness::Missing
        } else if required().any(|(c, _)| c.outcome == Some(Outcome::Insufficient)) {
            // Too few (or no) requests in the window: no conclusion, never a failure.
            Freshness::Insufficient
        } else {
            Freshness::Stale
        };
        let worst = self
            .conditions
            .iter()
            .zip(&states)
            .filter(|(c, s)| **s == Freshness::Current && c.outcome == Some(Outcome::Breached))
            .filter_map(|(c, _)| c.yields)
            .max();
        let state = match worst {
            Some(y) => y.as_str(),
            None if required_current => "normal",
            None => "unknown",
        };
        (state, status)
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
                let mut v = json!({
                    "signal": c.signal,
                    "last_evidence_at": c.last_evidence_at,
                    "evidence_status": condition_freshness(c, self.freshness_seconds, now).as_str(),
                });
                if c.kind != ConditionKind::Presence {
                    v["kind"] = json!(c.kind);
                    v["required"] = json!(c.required);
                    v["outcome"] = json!(c.outcome.map(Outcome::as_str));
                    v["value"] = json!(c.value);
                    v["requests"] = json!(c.requests);
                    v["yields"] = json!(c.yields.map(Yields::as_str));
                }
                v
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

/// Measures one threshold condition over the server requests of every identity in scope.
async fn threshold_evidence(
    evidence: &Evidence,
    tenant_id: Uuid,
    identities: &[TelemetryIdentity],
    c: &Condition,
    started_at: DateTime<Utc>,
    until_ns: u64,
) -> Option<ConditionEvidence> {
    let window = Duration::seconds(i64::from(c.window_seconds?));
    let since_ns = u64::try_from((started_at - window).timestamp_nanos_opt()?).ok()?;
    let level = c.quantile.unwrap_or(0.5);
    let stats = evidence
        .window_stats(tenant_id, identities, since_ns, until_ns, level)
        .await
        .ok()?;
    let threshold = c.threshold?;
    let measured = if stats.requests < c.min_requests? {
        None
    } else {
        match c.kind {
            ConditionKind::ErrorRatio => Some(stats.errors as f64 / stats.requests as f64),
            _ => stats.quantile_ns.map(|ns| ns / 1_000_000.0),
        }
    };
    let outcome = match measured {
        None => Outcome::Insufficient,
        Some(v) if c.kind == ConditionKind::ErrorRatio && v >= threshold => Outcome::Breached,
        Some(v) if c.kind == ConditionKind::LatencyQuantile && v > threshold => Outcome::Breached,
        Some(_) => Outcome::Ok,
    };
    Some(ConditionEvidence {
        kind: c.kind,
        signal: c.signal.clone(),
        classification: c.classification.clone(),
        last_evidence_at: stats.last_seen.and_then(ns_to_time),
        required: c.required,
        outcome: Some(outcome),
        value: measured,
        requests: Some(stats.requests),
        yields: c.effective_yields(),
    })
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
    let until_ns = u64::try_from(started_at.timestamp_nanos_opt()?).ok()?;
    let mut conditions = Vec::with_capacity(rule.conditions.len());
    for c in &rule.conditions {
        let evidence = match c.kind {
            ConditionKind::Presence => ConditionEvidence::presence(
                &c.signal,
                &c.classification,
                latest
                    .iter()
                    .filter_map(|t| signal_time(t, &c.signal))
                    .max()
                    .and_then(ns_to_time),
            ),
            ConditionKind::ErrorRatio | ConditionKind::LatencyQuantile => {
                threshold_evidence(evidence, tenant_id, &identities, c, started_at, until_ns)
                    .await?
            }
        };
        conditions.push(evidence);
    }
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
                .map(|age| {
                    ConditionEvidence::presence(
                        "any",
                        "internal",
                        age.map(|s| now - Duration::seconds(s)),
                    )
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

    fn threshold(
        outcome: Outcome,
        yields: Yields,
        age: Option<i64>,
        required: bool,
        now: DateTime<Utc>,
    ) -> ConditionEvidence {
        ConditionEvidence {
            kind: ConditionKind::ErrorRatio,
            signal: "traces".into(),
            classification: "internal".into(),
            last_evidence_at: age.map(|s| now - Duration::seconds(s)),
            required,
            outcome: Some(outcome),
            value: None,
            requests: Some(0),
            yields: Some(yields),
        }
    }

    #[test]
    fn worst_current_breach_wins_and_absence_is_never_a_breach() {
        use Outcome::*;
        use Yields::*;
        let now = Utc::now();
        let with = |extra: Vec<ConditionEvidence>| {
            let mut s = snapshot(vec![Some(10)], now);
            s.conditions.extend(extra);
            s.freshness(now)
        };
        assert_eq!(
            with(vec![threshold(Breached, Degraded, Some(5), true, now)]),
            ("degraded", Freshness::Current)
        );
        assert_eq!(
            with(vec![
                threshold(Breached, Degraded, Some(5), true, now),
                threshold(Breached, Unavailable, Some(5), true, now),
            ]),
            ("unavailable", Freshness::Current)
        );
        // A breach that aged out no longer counts.
        assert_eq!(
            with(vec![threshold(Breached, Unavailable, Some(301), true, now)]).0,
            "unknown"
        );
        // No requests: insufficient, never unavailable.
        assert_eq!(
            with(vec![threshold(Insufficient, Unavailable, None, true, now)]),
            ("unknown", Freshness::Insufficient)
        );
        assert_eq!(
            with(vec![threshold(Insufficient, Degraded, Some(5), true, now)]),
            ("unknown", Freshness::Insufficient)
        );
        // An optional threshold without enough requests does not block normal.
        assert_eq!(
            with(vec![threshold(Insufficient, Degraded, None, false, now)]),
            ("normal", Freshness::Current)
        );
        assert_eq!(
            with(vec![threshold(Ok, Unavailable, Some(5), true, now)]),
            ("normal", Freshness::Current)
        );
        // Stale required presence outranks insufficient.
        let mut s = snapshot(vec![Some(400)], now);
        s.conditions
            .push(threshold(Insufficient, Degraded, None, true, now));
        assert_eq!(s.freshness(now), ("unknown", Freshness::Stale));
    }

    #[test]
    fn rule_validation_requires_affirmative_bad_states() {
        let presence = Condition::presence("any", "internal");
        let ratio = |threshold: f64, yields: Option<Yields>| Condition {
            kind: ConditionKind::ErrorRatio,
            signal: "traces".into(),
            threshold: Some(threshold),
            window_seconds: Some(300),
            min_requests: Some(10),
            yields,
            ..Condition::presence("traces", "internal")
        };
        let latency = |yields: Option<Yields>| Condition {
            kind: ConditionKind::LatencyQuantile,
            quantile: Some(0.95),
            threshold: Some(500.0),
            yields,
            ..ratio(0.5, None)
        };
        let ok = |c: Vec<Condition>| validate_rule(300, 3600, &c);
        assert!(ok(vec![presence.clone(), ratio(0.05, Some(Yields::Degraded))]).is_ok());
        assert!(
            ok(vec![
                presence.clone(),
                ratio(1.0, Some(Yields::Unavailable))
            ])
            .is_ok()
        );
        assert!(ok(vec![presence.clone(), latency(None)]).is_ok());
        assert!(
            ok(vec![
                presence.clone(),
                ratio(0.5, Some(Yields::Unavailable))
            ])
            .is_err()
        );
        assert!(ok(vec![presence.clone(), ratio(0.5, None)]).is_err());
        assert!(ok(vec![presence.clone(), latency(Some(Yields::Unavailable))]).is_err());
        assert!(ok(vec![ratio(0.05, Some(Yields::Degraded))]).is_err());
        assert!(ok(vec![]).is_err());
        let short = Condition {
            window_seconds: Some(30),
            ..ratio(0.05, Some(Yields::Degraded))
        };
        assert!(ok(vec![presence.clone(), short]).is_err());
        let long = Condition {
            window_seconds: Some(7200),
            ..ratio(0.05, Some(Yields::Degraded))
        };
        assert!(ok(vec![presence.clone(), long]).is_err());
        let logs = Condition {
            signal: "logs".into(),
            ..ratio(0.05, Some(Yields::Degraded))
        };
        assert!(ok(vec![presence.clone(), logs]).is_err());
        let optional_presence = Condition {
            required: false,
            ..presence.clone()
        };
        assert!(ok(vec![optional_presence]).is_err());
        assert!(validate_rule(300, 200, std::slice::from_ref(&presence)).is_err());
        // The seeded platform default still parses and validates.
        let seeded: Vec<Condition> =
            serde_json::from_str(r#"[{"signal": "any", "classification": "internal"}]"#).unwrap();
        assert_eq!(seeded, vec![presence]);
        assert!(
            serde_json::from_str::<Vec<Condition>>(
                r#"[{"signal": "any", "classification": "internal", "bogus": 1}]"#
            )
            .is_err()
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
