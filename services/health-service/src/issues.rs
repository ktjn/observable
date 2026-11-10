//! Data-quality issues (phase 2 E4, OBS-26-02 and OBS-26-04).
//!
//! The evaluation worker reconciles issues per service and class on every pass:
//! - `source_stopped`: a bound identity had evidence within the lookback and then none for
//!   one freshness bound. It stays open while the identity is silent, and resolves when
//!   evidence returns or the identity is no longer bound.
//! - `inconsistent_inputs`: the catalog inputs for the service are ambiguous.
//! - `evaluation_failed`: the service's evidence could not be read.
//!
//! `GET /v1/health/services/{id}/issues?environment_class=` discloses open issues and those
//! resolved in the last 7 days, under `health.issue` with the shared read checks: denied
//! evidence withholds the issues as `restricted`.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::HealthState;
use crate::guard::{Refusal, caller, evidence_read};
use crate::ports::{EnvironmentClass, TelemetryIdentity};

pub const ISSUE: &str = "health.issue";
pub const SOURCE_STOPPED: &str = "source_stopped";
pub const INCONSISTENT_INPUTS: &str = "inconsistent_inputs";
pub const EVALUATION_FAILED: &str = "evaluation_failed";
pub const ALL_KINDS: &[&str] = &[SOURCE_STOPPED, INCONSISTENT_INPUTS, EVALUATION_FAILED];
/// How long resolved issues stay disclosed.
pub const RESOLVED_DAYS: i64 = 7;

/// One finding of an evaluation pass.
#[derive(Clone, Debug, PartialEq)]
pub struct Finding {
    pub kind: &'static str,
    pub service_name: String,
    pub environment: String,
    pub detail: serde_json::Value,
}

impl Finding {
    pub fn service(kind: &'static str) -> Self {
        Self {
            kind,
            service_name: String::new(),
            environment: String::new(),
            detail: json!({}),
        }
    }
}

/// Identities whose source has stopped: evidence within the lookback (`last`) but none for
/// one freshness bound, or silent while an issue for them is already open.
pub fn stopped_sources(
    sources: &[(TelemetryIdentity, Option<DateTime<Utc>>)],
    open: &[(String, String)],
    freshness_seconds: i32,
    at: DateTime<Utc>,
) -> Vec<Finding> {
    let bound = Duration::seconds(i64::from(freshness_seconds));
    sources
        .iter()
        .filter(|(identity, last)| match last {
            Some(t) => at - *t > bound,
            None => open.contains(&(identity.service_name.clone(), identity.environment.clone())),
        })
        .map(|(identity, last)| Finding {
            kind: SOURCE_STOPPED,
            service_name: identity.service_name.clone(),
            environment: identity.environment.clone(),
            detail: json!({ "last_evidence_at": last }),
        })
        .collect()
}

/// Identities with an open `source_stopped` issue.
pub async fn open_stopped(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
    class: EnvironmentClass,
) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT service_name, environment FROM health.data_quality_issues WHERE tenant_id = $1 \
         AND service_id = $2 AND environment_class = $3 AND kind = 'source_stopped' \
         AND resolved_at IS NULL",
    )
    .bind(tenant_id)
    .bind(service_id)
    .bind(class.as_str())
    .fetch_all(db)
    .await
}

/// Opens or refreshes each finding, and resolves open issues of `kinds` that were not found.
pub async fn reconcile(
    db: &PgPool,
    tenant_id: Uuid,
    service_id: Uuid,
    class: EnvironmentClass,
    kinds: &[&str],
    findings: &[Finding],
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let mut tx = db.begin().await?;
    for f in findings {
        sqlx::query(
            "INSERT INTO health.data_quality_issues (id, tenant_id, service_id, environment_class, \
             kind, service_name, environment, opened_at, last_seen_at, detail) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9) \
             ON CONFLICT (tenant_id, service_id, environment_class, kind, service_name, environment) \
             WHERE resolved_at IS NULL DO UPDATE SET \
             last_seen_at = GREATEST(health.data_quality_issues.last_seen_at, EXCLUDED.last_seen_at), \
             detail = EXCLUDED.detail",
        )
        .bind(Uuid::new_v4())
        .bind(tenant_id)
        .bind(service_id)
        .bind(class.as_str())
        .bind(f.kind)
        .bind(&f.service_name)
        .bind(&f.environment)
        .bind(at)
        .bind(&f.detail)
        .execute(&mut *tx)
        .await?;
    }
    let found: Vec<String> = findings
        .iter()
        .map(|f| format!("{}\u{1f}{}\u{1f}{}", f.kind, f.service_name, f.environment))
        .collect();
    let kinds: Vec<String> = kinds.iter().map(|k| (*k).to_owned()).collect();
    sqlx::query(
        "UPDATE health.data_quality_issues SET resolved_at = GREATEST($5, opened_at) \
         WHERE tenant_id = $1 AND service_id = $2 AND environment_class = $3 \
         AND resolved_at IS NULL AND kind = ANY($4) \
         AND NOT (kind || chr(31) || service_name || chr(31) || environment = ANY($6))",
    )
    .bind(tenant_id)
    .bind(service_id)
    .bind(class.as_str())
    .bind(&kinds)
    .bind(at)
    .bind(&found)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct Issue {
    pub id: Uuid,
    pub kind: String,
    pub service_name: String,
    pub environment: String,
    pub opened_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub detail: serde_json::Value,
}

#[derive(Deserialize)]
pub struct IssueQuery {
    environment_class: Option<String>,
}

fn withheld(id: Uuid, class: EnvironmentClass, evidence_status: &str) -> Response {
    Json(json!({
        "service_id": id,
        "environment_class": class.as_str(),
        "evidence_status": evidence_status,
        "issues": [],
    }))
    .into_response()
}

pub async fn get_issues(
    State(state): State<HealthState>,
    headers: HeaderMap,
    Path(raw_id): Path<String>,
    Query(q): Query<IssueQuery>,
) -> Response {
    let caller = match caller(&state, &headers).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(class) = q
        .environment_class
        .as_deref()
        .and_then(EnvironmentClass::parse)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid_request",
                "message": "environment_class must be production or non_production",
            })),
        )
            .into_response();
    };
    let id = match evidence_read(&state, &caller, &headers, &raw_id, ISSUE, class).await {
        Ok((id, _)) => id,
        Err(Refusal::Respond(r)) => return r,
        Err(Refusal::Withhold(status)) => {
            return withheld(raw_id.parse().unwrap_or_default(), class, status);
        }
    };
    let rows: Result<Vec<Issue>, _> = sqlx::query_as(
        "SELECT id, kind, service_name, environment, opened_at, last_seen_at, resolved_at, detail \
         FROM health.data_quality_issues WHERE tenant_id = $1 AND service_id = $2 \
         AND environment_class = $3 AND (resolved_at IS NULL OR resolved_at >= $4) \
         ORDER BY resolved_at IS NOT NULL, opened_at DESC LIMIT 200",
    )
    .bind(caller.tenant_id)
    .bind(id)
    .bind(class.as_str())
    .bind(Utc::now() - Duration::days(RESOLVED_DAYS))
    .fetch_all(&state.db)
    .await;
    match rows {
        Ok(issues) => Json(json!({
            "service_id": id,
            "environment_class": class.as_str(),
            "evidence_status": "current",
            "open": issues.iter().filter(|i| i.resolved_at.is_none()).count(),
            "issues": issues,
        }))
        .into_response(),
        Err(e) => {
            tracing::warn!(error = %e, "health issue store unavailable");
            withheld(id, class, "evaluation_unavailable")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> TelemetryIdentity {
        TelemetryIdentity {
            service_name: name.into(),
            environment: "prod".into(),
        }
    }

    #[test]
    fn a_source_stops_after_one_freshness_bound_and_stays_stopped_while_silent() {
        let now = Utc::now();
        let sources = vec![
            (id("fresh"), Some(now - Duration::seconds(299))),
            (id("stopped"), Some(now - Duration::seconds(301))),
            (id("never"), None),
            (id("still-silent"), None),
        ];
        let open = vec![("still-silent".to_owned(), "prod".to_owned())];
        let names: Vec<String> = stopped_sources(&sources, &open, 300, now)
            .into_iter()
            .map(|f| f.service_name)
            .collect();
        assert_eq!(names, ["stopped", "still-silent"]);
    }
}
