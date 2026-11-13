//! Platform confidence (phase 2 E6, OBS-29-03).
//!
//! `GET /v1/platform/confidence` composes the providers' self-statuses and the caller's tenant
//! ingest watermark into one disclosure:
//!
//! - `assess`: `GET {health}/internal/health/self-status`.
//! - `detect`: `GET {alerting}/internal/alerting/self-status`.
//! - `ingest`: the tenant's newest observation against `INGEST_WATERMARK_BOUND_SECONDS`.
//!
//! Each function is `current`, or names why not. A function that is `stale`, `delayed` or
//! whose status cannot be read is a loss of confidence (`degraded`, with the function named);
//! one that has not completed a pass yet, or a tenant with no recent telemetry, is `unknown`.
//! Confidence is `full` only when every function is current, never by default. The body holds
//! states and times only: no provider counts and no other tenant's data.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::Extension,
    http::HeaderName,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use observable_storage_contracts::watermark::{IngestWatermark, WatermarkStatus};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

/// Lookback for the watermark read. Older evidence is `missing`, which is never current.
const WATERMARK_LOOKBACK: Duration = Duration::from_secs(24 * 3600);
pub const DEFAULT_INGEST_BOUND: Duration = Duration::from_secs(300);
const INTERNAL_TOKEN: HeaderName = HeaderName::from_static("x-internal-token");

/// The tenant's ingest watermark. Errors are an unreadable store, reported as degraded.
#[async_trait::async_trait]
pub trait WatermarkSource: Send + Sync {
    async fn read(
        &self,
        tenant_id: Uuid,
        since_ns: u64,
        now_ns: u64,
    ) -> Result<IngestWatermark, ()>;
}

pub struct ClickHouseWatermark(pub clickhouse::Client);

#[async_trait::async_trait]
impl WatermarkSource for ClickHouseWatermark {
    async fn read(
        &self,
        tenant_id: Uuid,
        since_ns: u64,
        now_ns: u64,
    ) -> Result<IngestWatermark, ()> {
        observable_storage_contracts::watermark::ingest_watermark(
            &self.0, tenant_id, since_ns, now_ns,
        )
        .await
        .map_err(|e| tracing::warn!(error = %e, "ingest watermark unavailable"))
    }
}

/// Where the self-statuses and the watermark come from. `None` parts are not deployed.
#[derive(Clone, Default)]
pub struct PlatformSignals {
    inner: Option<Arc<Signals>>,
}

struct Signals {
    health: Option<String>,
    alerting: String,
    token: String,
    client: reqwest::Client,
    watermark: Arc<dyn WatermarkSource>,
    ingest_bound: Duration,
}

impl PlatformSignals {
    pub fn new(
        health: Option<String>,
        alerting: impl Into<String>,
        token: impl Into<String>,
        client: reqwest::Client,
        watermark: Arc<dyn WatermarkSource>,
        ingest_bound: Duration,
    ) -> Self {
        let trim = |s: String| s.trim_end_matches('/').to_string();
        Self {
            inner: Some(Arc::new(Signals {
                health: health.map(trim).filter(|h| !h.is_empty()),
                alerting: trim(alerting.into()),
                token: token.into(),
                client,
                watermark,
                ingest_bound,
            })),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct FunctionState {
    pub function: &'static str,
    /// `current`, `stale`, `not_started`, `delayed`, `missing` or `unavailable`.
    pub state: String,
    /// The last successful pass (assess, detect) or the newest observation (ingest).
    pub last_success_at: Option<DateTime<Utc>>,
}

impl FunctionState {
    fn new(function: &'static str, state: &str, last: Option<DateTime<Utc>>) -> Self {
        Self {
            function,
            state: state.into(),
            last_success_at: last,
        }
    }

    fn effect(&self) -> Effect {
        match self.state.as_str() {
            "current" => Effect::Full,
            "not_started" | "missing" => Effect::Unknown,
            _ => Effect::Degraded,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Effect {
    Full,
    Unknown,
    Degraded,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PlatformConfidence {
    /// `full`, `degraded` or `unknown`.
    pub confidence: &'static str,
    /// Functions behind a `degraded` or `unknown` confidence.
    pub affected: Vec<&'static str>,
    pub functions: Vec<FunctionState>,
    pub checked_at: DateTime<Utc>,
}

impl PlatformConfidence {
    pub fn is_full(&self) -> bool {
        self.confidence == "full"
    }
}

/// Combines function states: any degraded function wins, then any unknown one.
pub fn combine(functions: Vec<FunctionState>, checked_at: DateTime<Utc>) -> PlatformConfidence {
    let worst = functions
        .iter()
        .map(FunctionState::effect)
        .max()
        .unwrap_or(Effect::Unknown);
    let confidence = match worst {
        Effect::Full => "full",
        Effect::Unknown => "unknown",
        Effect::Degraded => "degraded",
    };
    let affected = if functions.is_empty() {
        vec!["assess", "detect", "ingest"]
    } else {
        functions
            .iter()
            .filter(|f| f.effect() == worst && worst != Effect::Full)
            .map(|f| f.function)
            .collect()
    };
    PlatformConfidence {
        confidence,
        affected,
        functions,
        checked_at,
    }
}

/// Maps a self-status body to the function's state. Unknown shapes are `unavailable`.
fn self_status(function: &'static str, body: Option<Value>) -> FunctionState {
    let Some(v) = body else {
        return FunctionState::new(function, "unavailable", None);
    };
    let state = v.get("state").and_then(Value::as_str);
    let last = v
        .get("last_success")
        .or_else(|| v.get("last_pass"))
        .and_then(|p| p.get("completed_at"))
        .and_then(Value::as_str)
        .and_then(|t| t.parse().ok());
    match state {
        Some(s @ ("current" | "stale" | "not_started")) => FunctionState::new(function, s, last),
        _ => FunctionState::new(function, "unavailable", None),
    }
}

async fn fetch_status(s: &Signals, url: String) -> Option<Value> {
    let resp = s
        .client
        .get(url)
        .header(INTERNAL_TOKEN, &s.token)
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().await.ok()
}

fn ns(t: DateTime<Utc>) -> u64 {
    t.timestamp_nanos_opt().unwrap_or_default().max(0) as u64
}

async fn ingest(s: &Signals, tenant: Option<Uuid>, now: DateTime<Utc>) -> FunctionState {
    let Some(tenant) = tenant else {
        return FunctionState::new("ingest", "unavailable", None);
    };
    let now_ns = ns(now);
    let since = now_ns.saturating_sub(WATERMARK_LOOKBACK.as_nanos() as u64);
    match s.watermark.read(tenant, since, now_ns).await {
        Err(()) => FunctionState::new("ingest", "unavailable", None),
        Ok(w) => {
            let last = w.latest().map(|n| DateTime::from_timestamp_nanos(n as i64));
            let state = match w.status(now_ns, s.ingest_bound.as_nanos() as u64) {
                WatermarkStatus::Current => "current",
                WatermarkStatus::Delayed => "delayed",
                WatermarkStatus::Missing => "missing",
            };
            FunctionState::new("ingest", state, last)
        }
    }
}

/// Reads every function for the caller's tenant.
pub async fn assess(signals: Option<&PlatformSignals>, tenant: Option<Uuid>) -> PlatformConfidence {
    let now = Utc::now();
    let Some(s) = signals.and_then(|p| p.inner.as_deref()) else {
        return combine(Vec::new(), now);
    };
    let health = async {
        match &s.health {
            Some(h) => self_status(
                "assess",
                fetch_status(s, format!("{h}/internal/health/self-status")).await,
            ),
            None => FunctionState::new("assess", "unavailable", None),
        }
    };
    let alerting = async {
        self_status(
            "detect",
            fetch_status(s, format!("{}/internal/alerting/self-status", s.alerting)).await,
        )
    };
    let (assess, detect, ingest) = tokio::join!(health, alerting, ingest(s, tenant, now));
    combine(vec![assess, detect, ingest], now)
}

/// GET /v1/platform/confidence
pub async fn get_platform_confidence(
    signals: Option<Extension<PlatformSignals>>,
    tenant: Option<Extension<TenantContext>>,
) -> Response {
    let tenant = tenant.map(|Extension(t)| t.tenant_id);
    Json(assess(signals.as_ref().map(|Extension(s)| s), tenant).await).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(function: &'static str, state: &str) -> FunctionState {
        FunctionState::new(function, state, None)
    }

    #[test]
    fn confidence_is_full_only_when_every_function_is_current() {
        let now = Utc::now();
        let c = combine(
            vec![
                f("assess", "current"),
                f("detect", "current"),
                f("ingest", "current"),
            ],
            now,
        );
        assert_eq!((c.confidence, c.affected.len()), ("full", 0));

        let c = combine(
            vec![
                f("assess", "current"),
                f("detect", "stale"),
                f("ingest", "missing"),
            ],
            now,
        );
        assert_eq!((c.confidence, c.affected), ("degraded", vec!["detect"]));

        let c = combine(
            vec![
                f("assess", "not_started"),
                f("detect", "current"),
                f("ingest", "current"),
            ],
            now,
        );
        assert_eq!((c.confidence, c.affected), ("unknown", vec!["assess"]));

        let c = combine(Vec::new(), now);
        assert_eq!(c.confidence, "unknown");
    }

    #[test]
    fn unreadable_or_unexpected_self_status_is_unavailable() {
        assert_eq!(self_status("assess", None).state, "unavailable");
        let odd = serde_json::json!({ "state": "healthy" });
        assert_eq!(self_status("assess", Some(odd)).state, "unavailable");
        let ok = serde_json::json!({
            "state": "current",
            "last_pass": { "completed_at": "2026-11-10T12:00:00Z" },
        });
        let s = self_status("assess", Some(ok));
        assert_eq!(s.state, "current");
        assert!(s.last_success_at.is_some());
    }
}
