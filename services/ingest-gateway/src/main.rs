mod auth;
mod cardinality;
mod change_events;
mod deployment_registry;
mod deployments;
mod grpc;
#[path = "http-json/mod.rs"]
mod http_json;
mod prometheus_rw;
mod queue;
mod readyz;

use deployment_registry::DeploymentRegistry;
use observable_auth::{ApiKeyContext, AuthError};
use sqlx::postgres::PgPoolOptions;
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
    pub db: Arc<sqlx::PgPool>,
    pub deployment_registry: Arc<DeploymentRegistry>,
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
    pub async fn validate_api_key(&self, key: &str) -> Result<ApiKeyContext, AuthError> {
        #[cfg(test)]
        if let Some(id) = self.stub_tenant {
            if key == "dev-api-key-0000" {
                return Ok(ApiKeyContext {
                    tenant_id: id,
                    role: "member".to_string(),
                    environment: "testbench".to_string(),
                });
            }
            if key == "dev-viewer-key-0000" {
                return Ok(ApiKeyContext {
                    tenant_id: id,
                    role: "viewer".to_string(),
                    environment: String::new(),
                });
            }
            return Err(AuthError::Unauthorized);
        }

        observable_auth::verify_api_key(&self.http_client, &self.auth_service_url, key).await
    }

    #[cfg(test)]
    pub fn test_stub() -> Self {
        let db = test_pool();
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(db.clone()),
            db,
            stub_tenant: None,
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth(tenant_id: &str) -> Self {
        let db = test_pool();
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(db.clone()),
            db,
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_rate_limit(tenant_id: &str, per_second: u32) -> Self {
        let db = test_pool();
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(per_second),
            log_rate_limiter: build_rate_limiter(per_second),
            metric_rate_limiter: build_rate_limiter(per_second),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(db.clone()),
            db,
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_metric_budget(tenant_id: &str, budget: u64) -> Self {
        let db = test_pool();
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: build_rate_limiter(1000),
            log_rate_limiter: build_rate_limiter(1000),
            metric_rate_limiter: build_rate_limiter(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(budget),
            deployment_registry: DeploymentRegistry::new(db.clone()),
            db,
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }
}

#[cfg(test)]
fn test_pool() -> Arc<sqlx::PgPool> {
    // Disconnected pool — unit tests that don't touch the DB can use this.
    Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("ingest-gateway")?;

    let http_port: u16 = std::env::var("INGEST_GATEWAY_HTTP_JSON_PORT")
        .or_else(|_| std::env::var("INGEST_GATEWAY_PORT"))
        .unwrap_or_else(|_| "4318".into())
        .parse()?;
    let grpc_port: u16 = std::env::var("INGEST_GATEWAY_GRPC_PORT")
        .unwrap_or_else(|_| "4317".into())
        .parse()?;
    let platform_port: u16 = std::env::var("INGEST_GATEWAY_PLATFORM_PORT")
        .unwrap_or_else(|_| "4321".into())
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
    let grpc_max_message_bytes: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4_194_304);

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://observable:observable@localhost:5432/observable".into());
    let db = Arc::new(
        PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await?,
    );

    let state = AppState {
        auth_service_url: std::env::var("AUTH_SERVICE_URL")
            .unwrap_or_else(|_| "http://localhost:4319".into()),
        http_client: reqwest::Client::new(),
        producer: Some(producer),
        trace_rate_limiter: build_rate_limiter(trace_rate_limit),
        log_rate_limiter: build_rate_limiter(log_rate_limit),
        metric_rate_limiter: build_rate_limiter(metric_rate_limit),
        metric_cardinality: cardinality::MetricCardinalityBudget::new(metric_series_budget),
        deployment_registry: DeploymentRegistry::new(db.clone()),
        db: db.clone(),
        #[cfg(test)]
        stub_tenant: None,
    };

    let grpc_state = state.clone();
    let platform_state = state.clone();
    let probe_state = readyz::IngestGatewayProbeState { db: db.clone() };
    let grpc_future = grpc::start_grpc_server(grpc_state, grpc_port, grpc_max_message_bytes);
    let http_future = http_json::start_http_server(state, http_port);
    let platform_future =
        http_json::start_platform_server(platform_state, probe_state, platform_port);

    tokio::select! {
        res = grpc_future => res?,
        res = http_future => res?,
        res = platform_future => res?,
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn grpc_max_message_bytes_defaults_to_4mib() {
        unsafe {
            std::env::remove_var("INGEST_GRPC_MAX_MESSAGE_BYTES");
        }
        let val: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_194_304);
        assert_eq!(val, 4_194_304);
    }

    #[test]
    fn grpc_max_message_bytes_parses_env_var() {
        unsafe {
            std::env::set_var("INGEST_GRPC_MAX_MESSAGE_BYTES", "8388608");
        }
        let val: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(4_194_304);
        unsafe {
            std::env::remove_var("INGEST_GRPC_MAX_MESSAGE_BYTES");
        }
        assert_eq!(val, 8_388_608);
    }
}
