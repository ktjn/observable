mod auth;
mod cardinality;
mod grpc;
mod queue;
mod routes;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::num::NonZeroU32;
use std::sync::Arc;
use uuid::Uuid;

use queue::producer::QueueProducer;

#[derive(Clone)]
pub struct AppState {
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub producer: Option<Arc<QueueProducer>>,
    pub trace_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub log_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub metric_rate_limiter: Arc<governor::DefaultKeyedRateLimiter<Uuid>>,
    pub metric_cardinality: Arc<cardinality::MetricCardinalityBudget>,
    #[cfg(test)]
    pub stub_tenant: Option<Uuid>,
}

fn build_rate_limiter(per_second: u32) -> Arc<governor::DefaultKeyedRateLimiter<Uuid>> {
    let quota = governor::Quota::per_second(
        NonZeroU32::new(per_second).expect("rate limit per second must be > 0"),
    );
    Arc::new(governor::RateLimiter::keyed(quota))
}

impl AppState {
    pub async fn validate_api_key(&self, key: &str) -> anyhow::Result<(Uuid, String)> {
        #[cfg(test)]
        if let Some(id) = self.stub_tenant {
            if key == "dev-api-key-0000" {
                return Ok((id, "member".to_string()));
            }
            if key == "dev-viewer-key-0000" {
                return Ok((id, "viewer".to_string()));
            }
            anyhow::bail!("stub auth rejected");
        }

        let resp = self
            .http_client
            .post(format!("{}/internal/validate", self.auth_service_url))
            .json(&serde_json::json!({"api_key": key}))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("auth rejected");
        }
        let body: serde_json::Value = resp.json().await?;
        let id = body["tenant_id"].as_str().unwrap_or_default().parse()?;
        let role = body["role"].as_str().unwrap_or("member").to_string();
        Ok((id, role))
    }

    #[cfg(test)]
    pub fn test_stub() -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            stub_tenant: None,
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth(tenant_id: &str) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_rate_limit(tenant_id: &str, per_second: u32) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(per_second),
            log_rate_limiter: build_rate_limiter(per_second),
            metric_rate_limiter: build_rate_limiter(per_second),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_metric_budget(tenant_id: &str, budget: u64) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(budget),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/v1/traces", post(routes::traces::export_traces))
        .route("/v1/logs", post(routes::logs::export_logs))
        .route("/v1/metrics", post(routes::metrics::export_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    domain::telemetry::init_self_observability_telemetry("ingest-gateway")?;

    let http_port: u16 = std::env::var("INGEST_GATEWAY_PORT")
        .unwrap_or_else(|_| "4318".into())
        .parse()?;
    let grpc_port: u16 = std::env::var("INGEST_GATEWAY_GRPC_PORT")
        .unwrap_or_else(|_| "4317".into())
        .parse()?;

    let brokers = std::env::var("REDPANDA_BROKERS").unwrap_or_else(|_| "localhost:9092".into());
    let topic = std::env::var("INGEST_TOPIC").unwrap_or_else(|_| "telemetry.raw".into());
    let producer = Arc::new(QueueProducer::new(&brokers, &topic)?);
    let trace_rate_limit: u32 = std::env::var("TRACE_INGEST_RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let log_rate_limit: u32 = std::env::var("LOG_INGEST_RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let metric_rate_limit: u32 = std::env::var("METRIC_INGEST_RATE_LIMIT_PER_SECOND")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let metric_series_budget: u64 = std::env::var("METRIC_SERIES_BUDGET_PER_TENANT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    let state = AppState {
        auth_service_url: std::env::var("AUTH_SERVICE_URL")
            .unwrap_or_else(|_| "http://localhost:4319".into()),
        http_client: reqwest::Client::new(),
        producer: Some(producer),
        trace_rate_limiter: build_rate_limiter(trace_rate_limit),
        log_rate_limiter: build_rate_limiter(log_rate_limit),
        metric_rate_limiter: build_rate_limiter(metric_rate_limit),
        metric_cardinality: cardinality::MetricCardinalityBudget::new(metric_series_budget),
        #[cfg(test)]
        stub_tenant: None,
    };

    let app = build_router(state.clone());
    let http_listener = tokio::net::TcpListener::bind(("0.0.0.0", http_port)).await?;
    tracing::info!(port = http_port, "ingest-gateway HTTP listening");

    let grpc_state = state.clone();
    let grpc_future = grpc::start_grpc_server(grpc_state, grpc_port);
    let http_future = axum::serve(http_listener, app);

    tokio::select! {
        res = grpc_future => res?,
        res = http_future => res.map_err(anyhow::Error::from)?,
    }

    Ok(())
}
