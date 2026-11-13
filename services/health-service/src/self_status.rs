//! Health self-status (phase 2 E4, OBS-29-03 input).
//!
//! `GET /internal/health/self-status` (internal token) reports whether the evaluation worker
//! is keeping up: the last completed pass and its counts, with no tenant data. The pass is
//! `stale` once it is older than twice the worker interval, so a stopped or stuck worker is
//! visible even while the process still answers.

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

use crate::HealthState;
use crate::assessment::PassSummary;

#[derive(Clone, Copy, Debug)]
pub struct Pass {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub tenants: usize,
    pub summary: PassSummary,
}

/// Shared between the worker and the endpoint.
#[derive(Debug, Default)]
pub struct WorkerStatus {
    inner: Mutex<(Option<std::time::Duration>, Option<Pass>)>,
}

impl WorkerStatus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn started(&self, interval: std::time::Duration) {
        self.inner.lock().expect("status poisoned").0 = Some(interval);
    }

    pub fn completed(&self, pass: Pass) {
        self.inner.lock().expect("status poisoned").1 = Some(pass);
    }

    /// `not_started`, `current` or `stale` at `now`, with the report body.
    pub fn report(&self, now: DateTime<Utc>) -> serde_json::Value {
        let (interval, pass) = *self.inner.lock().expect("status poisoned");
        let state = match (interval, pass) {
            (None, _) | (_, None) => "not_started",
            (Some(i), Some(p)) => {
                let bound = Duration::from_std(i * 2).unwrap_or(Duration::MAX);
                if now - p.completed_at > bound {
                    "stale"
                } else {
                    "current"
                }
            }
        };
        json!({
            "component": "observable-health",
            "function": "assess",
            "state": state,
            "checked_at": now,
            "interval_seconds": interval.map(|i| i.as_secs()),
            "last_pass": pass.map(|p| json!({
                "started_at": p.started_at,
                "completed_at": p.completed_at,
                "tenants": p.tenants,
                "stored": p.summary.stored,
                "superseded": p.summary.superseded,
                "failed": p.summary.failed,
            })),
        })
    }
}

pub async fn get_self_status(State(state): State<HealthState>, headers: HeaderMap) -> Response {
    // An unset token refuses every caller.
    if state.internal_token.trim().is_empty()
        || observable_auth::verify_internal_token(&headers, &state.internal_token).is_err()
    {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
    }
    Json(state.worker.report(Utc::now())).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pass_older_than_twice_the_interval_is_stale() {
        let s = WorkerStatus::new();
        let now = Utc::now();
        assert_eq!(s.report(now)["state"], "not_started");
        s.started(std::time::Duration::from_secs(60));
        assert_eq!(s.report(now)["state"], "not_started");
        s.completed(Pass {
            started_at: now - Duration::seconds(5),
            completed_at: now,
            tenants: 2,
            summary: PassSummary::default(),
        });
        assert_eq!(s.report(now)["state"], "current");
        assert_eq!(s.report(now + Duration::seconds(119))["state"], "current");
        assert_eq!(s.report(now + Duration::seconds(121))["state"], "stale");
    }
}
