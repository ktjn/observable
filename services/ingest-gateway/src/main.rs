mod auth;
mod queue;
mod routes;

use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use uuid::Uuid;

use queue::producer::QueueProducer;

#[derive(Clone)]
pub struct AppState {
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub producer: Option<Arc<QueueProducer>>,
    #[cfg(test)]
    pub stub_tenant: Option<Uuid>,
}

impl AppState {
    pub async fn validate_api_key(&self, key: &str) -> anyhow::Result<Uuid> {
        #[cfg(test)]
        if let Some(id) = self.stub_tenant {
            if key == "dev-api-key-0000" {
                return Ok(id);
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
        Ok(id)
    }

    #[cfg(test)]
    pub fn test_stub() -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            stub_tenant: None,
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth(tenant_id: &str) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/v1/traces", post(routes::traces::export_traces))
        .route("/v1/logs", post(routes::logs::export_logs))
        .route("/v1/metrics", post(routes::metrics::export_metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    domain::telemetry::init_telemetry("ingest-gateway", otlp.as_deref())?;
    let port: u16 = std::env::var("INGEST_GATEWAY_PORT")
        .unwrap_or_else(|_| "4317".into())
        .parse()?;
    let brokers = std::env::var("REDPANDA_BROKERS").unwrap_or_else(|_| "localhost:9092".into());
    let topic = std::env::var("INGEST_TOPIC").unwrap_or_else(|_| "telemetry.raw".into());
    let producer = Arc::new(QueueProducer::new(&brokers, &topic)?);
    let state = AppState {
        auth_service_url: std::env::var("AUTH_SERVICE_URL")
            .unwrap_or_else(|_| "http://localhost:4318".into()),
        http_client: reqwest::Client::new(),
        producer: Some(producer),
        #[cfg(test)]
        stub_tenant: None,
    };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "ingest-gateway listening");
    axum::serve(listener, app).await?;
    Ok(())
}
