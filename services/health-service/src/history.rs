//! State transitions and derived interruptions (phase 2 E3).
//!
//! - `record` writes a transition in the snapshot's transaction whenever the stored state
//!   changes. An evaluation gap longer than the freshness bound first records a transition to
//!   `unknown` (`stale`) at the moment the previous evidence aged out, so a service that went
//!   unobserved is never shown as continuously failing or healthy.
//! - `interruptions` derives degradations and interruptions from transitions: onset, worst
//!   state, end and duration. An end or onset that falls inside an evidence gap is given as
//!   bounds, never as an exact time.
//! - `GET /v1/health/services/{id}/history?environment_class=&since=&until=` discloses both
//!   with the same checks as the current health read, under `health.history`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::HealthState;
use crate::assessment::{Snapshot, load};
use crate::ports::EnvironmentClass;

pub const HISTORY: &str = "health.history";
/// Default and longest history period.
pub const DEFAULT_PERIOD_DAYS: i64 = 7;
pub const MAX_PERIOD_DAYS: i64 = 90;

/// The previously stored assessment, read under the row lock.
pub struct Previous {
    pub state: String,
    pub evaluated_at: DateTime<Utc>,
    pub freshness_seconds: i32,
    pub rule_id: String,
    pub rule_revision: i32,
}

#[allow(clippy::too_many_arguments)]
async fn insert(
    tx: &mut Transaction<'_, Postgres>,
    s: &Snapshot,
    at: DateTime<Utc>,
    from: Option<&str>,
    to: &str,
    evidence_status: &str,
    rule_id: &str,
    rule_revision: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO health.transitions (tenant_id, service_id, environment_class, at, \
         from_state, to_state, evidence_status, rule_id, rule_revision) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) ON CONFLICT DO NOTHING",
    )
    .bind(s.tenant_id)
    .bind(s.service_id)
    .bind(s.environment_class.as_str())
    .bind(at)
    .bind(from)
    .bind(to)
    .bind(evidence_status)
    .bind(rule_id)
    .bind(rule_revision)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Records the transitions from `prev` to the newly stored snapshot `s`.
pub async fn record(
    tx: &mut Transaction<'_, Postgres>,
    s: &Snapshot,
    state: &str,
    evidence_status: &str,
    prev: Option<Previous>,
) -> Result<(), sqlx::Error> {
    let mut from = prev.as_ref().map(|p| p.state.clone());
    if let Some(p) = &prev {
        let aged_out = p.evaluated_at + Duration::seconds(i64::from(p.freshness_seconds));
        if p.state != "unknown" && aged_out < s.evaluated_at {
            insert(
                tx,
                s,
                aged_out,
                Some(&p.state),
                "unknown",
                "stale",
                &p.rule_id,
                p.rule_revision,
            )
            .await?;
            from = Some("unknown".into());
        }
    }
    if from.as_deref() != Some(state) {
        insert(
            tx,
            s,
            s.evaluated_at,
            from.as_deref(),
            state,
            evidence_status,
            &s.rule_id,
            s.rule_revision,
        )
        .await?;
    }
    Ok(())
}

/// Deletes transitions older than the retention period. Returns rows deleted.
pub async fn prune(db: &PgPool, retention: Duration) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query("DELETE FROM health.transitions WHERE at < $1")
        .bind(Utc::now() - retention)
        .execute(db)
        .await?
        .rows_affected())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct Transition {
    pub at: DateTime<Utc>,
    pub from_state: Option<String>,
    pub to_state: String,
    pub evidence_status: String,
}

fn is_bad(state: &str) -> bool {
    matches!(state, "degraded" | "unavailable")
}

fn rank(state: &str) -> u8 {
    match state {
        "unavailable" => 2,
        "degraded" => 1,
        _ => 0,
    }
}

/// A degradation or interruption derived from transitions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Interruption {
    /// First evaluation that showed the failing state.
    pub started_at: DateTime<Utc>,
    /// False when the failure follows unknown or no evidence: it began at or before
    /// `started_at`, after `started_after` when known.
    pub onset_exact: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_after: Option<DateTime<Utc>>,
    /// `unavailable` or `degraded`, the worst seen.
    pub worst_state: String,
    /// `resolved` once evidence shows `normal`, else `unresolved`.
    pub status: &'static str,
    /// Exact end: the evaluation that showed `normal` right after failing evidence.
    pub ended_at: Option<DateTime<Utc>>,
    /// When evidence was missing before recovery, the end lies in `[ended_after, ended_before]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_after: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_before: Option<DateTime<Utc>>,
    /// Only when onset and end are both exact.
    pub duration_seconds: Option<i64>,
    /// Evidence was missing at some point during the interruption.
    pub evidence_gap: bool,
}

struct Open {
    started_at: DateTime<Utc>,
    onset_exact: bool,
    started_after: Option<DateTime<Utc>>,
    worst: String,
    unknown_since: Option<DateTime<Utc>>,
    gap: bool,
}

/// Derives interruptions from transitions in ascending time order.
pub fn interruptions(transitions: &[Transition]) -> Vec<Interruption> {
    let mut out = vec![];
    let mut open: Option<Open> = None;
    // When the latest state before a failure was unknown (or there was none).
    let mut unknown_since: Option<DateTime<Utc>> = None;
    let mut seen_known = false;
    for t in transitions {
        let to = t.to_state.as_str();
        if open.is_none() {
            if is_bad(to) {
                let onset_exact = seen_known && unknown_since.is_none();
                open = Some(Open {
                    started_at: t.at,
                    onset_exact,
                    started_after: if onset_exact { None } else { unknown_since },
                    worst: to.into(),
                    unknown_since: None,
                    gap: !onset_exact,
                });
            }
        } else if is_bad(to) {
            let o = open.as_mut().expect("open");
            if rank(to) > rank(&o.worst) {
                o.worst = to.into();
            }
            o.unknown_since = None;
        } else if to == "normal" {
            let o = open.take().expect("open");
            let exact_end = o.unknown_since.is_none();
            out.push(Interruption {
                started_at: o.started_at,
                onset_exact: o.onset_exact,
                started_after: o.started_after,
                worst_state: o.worst,
                status: "resolved",
                ended_at: exact_end.then_some(t.at),
                ended_after: o.unknown_since,
                ended_before: (!exact_end).then_some(t.at),
                duration_seconds: (exact_end && o.onset_exact)
                    .then(|| (t.at - o.started_at).num_seconds()),
                evidence_gap: o.gap || !exact_end,
            });
        } else {
            // Unknown (or any other non-failing, non-normal state): the end is uncertain.
            let o = open.as_mut().expect("open");
            if o.unknown_since.is_none() {
                o.unknown_since = Some(t.at);
            }
            o.gap = true;
        }
        if to == "unknown" {
            unknown_since.get_or_insert(t.at);
        } else {
            unknown_since = None;
            seen_known = true;
        }
    }
    if let Some(o) = open {
        out.push(Interruption {
            started_at: o.started_at,
            onset_exact: o.onset_exact,
            started_after: o.started_after,
            worst_state: o.worst,
            status: "unresolved",
            ended_at: None,
            ended_after: None,
            ended_before: None,
            duration_seconds: None,
            evidence_gap: o.gap || o.unknown_since.is_some(),
        });
    }
    out
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    environment_class: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
}

fn invalid(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid_request", "message": message })),
    )
        .into_response()
}

fn withheld(
    id: Uuid,
    class: EnvironmentClass,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    evidence_status: &str,
) -> Response {
    Json(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "since": since,
        "until": until,
        "evidence_status": evidence_status,
        "transitions": [],
        "interruptions": [],
    }))
    .into_response()
}

/// Transitions in `[since, until]`, plus those since the last recovery before `since`, so an
/// interruption that began earlier keeps its onset.
async fn transitions(
    db: &PgPool,
    tenant_id: Uuid,
    id: Uuid,
    class: EnvironmentClass,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Result<Vec<Transition>, sqlx::Error> {
    sqlx::query_as(
        "SELECT at, from_state, to_state, evidence_status FROM health.transitions \
         WHERE tenant_id = $1 AND service_id = $2 AND environment_class = $3 AND at <= $5 \
         AND at >= COALESCE((SELECT max(at) FROM health.transitions WHERE tenant_id = $1 \
           AND service_id = $2 AND environment_class = $3 AND at < $4 AND to_state = 'normal'), $4) \
         ORDER BY at",
    )
    .bind(tenant_id)
    .bind(id)
    .bind(class.as_str())
    .bind(since)
    .bind(until)
    .fetch_all(db)
    .await
}

pub async fn get_history(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Response {
    let caller = match crate::guard::caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(class) = q
        .environment_class
        .as_deref()
        .and_then(EnvironmentClass::parse)
    else {
        return invalid("environment_class must be production or non_production");
    };
    let now = Utc::now();
    let until = q.until.unwrap_or(now).min(now);
    let since = q
        .since
        .unwrap_or(until - Duration::days(DEFAULT_PERIOD_DAYS));
    if since >= until {
        return invalid("since must be before until");
    }
    if until - since > Duration::days(MAX_PERIOD_DAYS) {
        return invalid("the period is at most 90 days");
    }

    // History stays readable for retired services the caller can still see.
    let id = match crate::guard::evidence_read(&state, &caller, &headers, &raw_id, HISTORY, class)
        .await
    {
        Ok((id, _)) => id,
        Err(crate::guard::Refusal::Respond(r)) => return r,
        Err(crate::guard::Refusal::Withhold(status)) => {
            let id = raw_id.parse().unwrap_or_default();
            return withheld(id, class, since, until, status);
        }
    };

    let (mut rows, snapshot) = match tokio::try_join!(
        transitions(&state.db, caller.tenant_id, id, class, since, until),
        load(&state.db, caller.tenant_id, id, class),
    ) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "health history store unavailable");
            return withheld(id, class, since, until, "evaluation_unavailable");
        }
    };
    // The worker has not evaluated since the last snapshot aged out: from then on the state
    // is unknown, which the stored transitions cannot show yet.
    if let (Some(s), Some(last)) = (&snapshot, rows.last()) {
        let aged_out = s.evaluated_at + Duration::seconds(i64::from(s.freshness_seconds));
        if last.to_state != "unknown" && aged_out < now && aged_out <= until {
            rows.push(Transition {
                at: aged_out,
                from_state: Some(last.to_state.clone()),
                to_state: "unknown".into(),
                evidence_status: "stale".into(),
            });
        }
    }
    let derived: Vec<Interruption> = interruptions(&rows)
        .into_iter()
        .filter(|i| {
            let end = i.ended_at.or(i.ended_before);
            i.started_at <= until && end.is_none_or(|e| e >= since)
        })
        .collect();
    let disclosed: Vec<&Transition> = rows.iter().filter(|t| t.at >= since).collect();
    Json(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "since": since,
        "until": until,
        "evidence_status": "current",
        "transitions": disclosed,
        "interruptions": derived,
    }))
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(min: i64, to: &str) -> Transition {
        Transition {
            at: DateTime::from_timestamp(1_700_000_000 + min * 60, 0).unwrap(),
            from_state: None,
            to_state: to.into(),
            evidence_status: "current".into(),
        }
    }

    #[test]
    fn a_known_outage_has_onset_duration_and_resolution() {
        let rows = [
            t(0, "normal"),
            t(10, "degraded"),
            t(15, "unavailable"),
            t(20, "degraded"),
            t(40, "normal"),
        ];
        let i = interruptions(&rows);
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].worst_state, "unavailable");
        assert_eq!(i[0].status, "resolved");
        assert_eq!(i[0].started_at, rows[1].at);
        assert_eq!(i[0].ended_at, Some(rows[4].at));
        assert_eq!(i[0].duration_seconds, Some(30 * 60));
        assert!(i[0].onset_exact && !i[0].evidence_gap);
    }

    #[test]
    fn a_missing_end_is_unresolved_or_bounded() {
        // Still failing.
        let i = interruptions(&[t(0, "normal"), t(10, "unavailable")]);
        assert_eq!((i[0].status, i[0].duration_seconds), ("unresolved", None));
        assert_eq!(i[0].ended_at, None);
        // Evidence lost, then normal: the end is only bounded.
        let rows = [
            t(0, "normal"),
            t(10, "unavailable"),
            t(20, "unknown"),
            t(50, "normal"),
        ];
        let i = interruptions(&rows);
        assert_eq!(i[0].status, "resolved");
        assert_eq!(i[0].ended_at, None);
        assert_eq!(
            (i[0].ended_after, i[0].ended_before),
            (Some(rows[2].at), Some(rows[3].at))
        );
        assert_eq!(i[0].duration_seconds, None);
        assert!(i[0].evidence_gap);
        // Evidence lost and never regained.
        let i = interruptions(&[t(0, "normal"), t(10, "degraded"), t(20, "unknown")]);
        assert_eq!((i[0].status, i[0].evidence_gap), ("unresolved", true));
    }

    #[test]
    fn an_onset_after_unknown_is_not_exact() {
        let rows = [t(0, "unknown"), t(10, "unavailable"), t(20, "normal")];
        let i = interruptions(&rows);
        assert!(!i[0].onset_exact);
        assert_eq!(i[0].started_after, Some(rows[0].at));
        assert_eq!(i[0].ended_at, Some(rows[2].at));
        assert_eq!(i[0].duration_seconds, None);
        // The very first evaluation already failing.
        let i = interruptions(&[t(0, "degraded"), t(5, "normal")]);
        assert!(!i[0].onset_exact);
        assert_eq!(i[0].started_after, None);
    }

    #[test]
    fn unknown_alone_is_never_an_interruption() {
        assert!(interruptions(&[t(0, "normal"), t(5, "unknown"), t(9, "normal")]).is_empty());
        assert!(interruptions(&[]).is_empty());
        // Two separate outages.
        let rows = [
            t(0, "normal"),
            t(1, "degraded"),
            t(2, "normal"),
            t(3, "unavailable"),
            t(4, "normal"),
        ];
        assert_eq!(interruptions(&rows).len(), 2);
    }
}
