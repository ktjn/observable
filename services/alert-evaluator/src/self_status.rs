//! Alerting self-status (phase 2 E5, OBS-29-03 input).
//!
//! `GET /internal/alerting/self-status` (internal token, `require_internal_service`) reports
//! whether the alert evaluation worker is keeping up, with counts only and no tenant data.
//! `state` is `stale` once the last *successful* pass is older than twice the interval, so a
//! paused, stuck or repeatedly failing evaluator is visible while the process still answers.

use std::sync::{Arc, Mutex};

use axum::{Extension, Json};
use chrono::{DateTime, Duration, Utc};
use serde_json::json;

#[derive(Debug, Default)]
struct Inner {
    interval: Option<std::time::Duration>,
    last_success: Option<(DateTime<Utc>, DateTime<Utc>)>,
    last_failure_at: Option<DateTime<Utc>>,
    consecutive_failures: u64,
}

/// Shared between the evaluation worker and the endpoint.
#[derive(Debug, Default)]
pub struct EvalStatus {
    inner: Mutex<Inner>,
}

impl EvalStatus {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn started(&self, interval: std::time::Duration) {
        self.inner.lock().expect("status poisoned").interval = Some(interval);
    }

    /// Records a completed pass that began at `started_at`.
    pub fn completed(&self, started_at: DateTime<Utc>, ok: bool) {
        let mut s = self.inner.lock().expect("status poisoned");
        let now = Utc::now();
        if ok {
            s.last_success = Some((started_at, now));
            s.consecutive_failures = 0;
        } else {
            s.last_failure_at = Some(now);
            s.consecutive_failures += 1;
        }
    }

    /// `not_started`, `current` or `stale` at `now`, with the report body.
    pub fn report(&self, now: DateTime<Utc>) -> serde_json::Value {
        let s = self.inner.lock().expect("status poisoned");
        let state = match (s.interval, s.last_success) {
            (None, _) | (_, None) => "not_started",
            (Some(i), Some((_, completed))) => {
                let bound = Duration::from_std(i * 2).unwrap_or(Duration::MAX);
                if now - completed > bound {
                    "stale"
                } else {
                    "current"
                }
            }
        };
        json!({
            "component": "observable-alerting",
            "function": "detect",
            "state": state,
            "checked_at": now,
            "interval_seconds": s.interval.map(|i| i.as_secs()),
            "last_success": s.last_success.map(|(started, completed)| json!({
                "started_at": started,
                "completed_at": completed,
            })),
            "last_failure_at": s.last_failure_at,
            "consecutive_failures": s.consecutive_failures,
        })
    }
}

/// GET /internal/alerting/self-status
pub async fn handle_self_status(
    Extension(status): Extension<Arc<EvalStatus>>,
) -> Json<serde_json::Value> {
    Json(status.report(Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_successful_passes_keep_the_evaluator_current() {
        let s = EvalStatus::new();
        let now = Utc::now();
        assert_eq!(s.report(now)["state"], "not_started");
        s.started(std::time::Duration::from_secs(60));
        s.completed(now, false);
        assert_eq!(s.report(now)["state"], "not_started");
        assert_eq!(s.report(now)["consecutive_failures"], 1);
        s.completed(now, true);
        let at = Utc::now();
        assert_eq!(s.report(at)["state"], "current");
        assert_eq!(s.report(at)["consecutive_failures"], 0);
        assert_eq!(s.report(at + Duration::seconds(119))["state"], "current");
        // Failures do not refresh the pass: it ages out.
        s.completed(at, false);
        assert_eq!(s.report(at + Duration::seconds(121))["state"], "stale");
    }
}
