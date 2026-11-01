mod auth;
mod cardinality;
mod deployment_registry;
mod grpc;
#[path = "http-json/mod.rs"]
mod http_json;
mod observability;
mod prometheus_rw;
mod queue;
mod rate_limiter;
mod readyz;

use deployment_registry::DeploymentRegistry;
use observable_auth::{ApiKeyContext, AuthError};
use std::sync::Arc;
#[cfg(test)]
use uuid::Uuid;

use queue::producer::QueueProducer;
use rate_limiter::TenantRateLimiter;

#[derive(Clone)]
pub struct AppState {
    pub auth_service_url: String,
    pub http_client: reqwest::Client,
    pub producer: Option<Arc<QueueProducer>>,
    pub trace_rate_limiter: Arc<TenantRateLimiter>,
    pub log_rate_limiter: Arc<TenantRateLimiter>,
    pub metric_rate_limiter: Arc<TenantRateLimiter>,
    pub metric_cardinality: Arc<cardinality::MetricCardinalityBudget>,
    pub deployment_registry: Arc<DeploymentRegistry>,
    pub metrics: Arc<observability::IngestGatewayMetrics>,
    #[cfg(test)]
    pub stub_tenant: Option<Uuid>,
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
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: TenantRateLimiter::new(1000),
            log_rate_limiter: TenantRateLimiter::new(1000),
            metric_rate_limiter: TenantRateLimiter::new(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(),
            metrics: Arc::new(observability::IngestGatewayMetrics::new()),
            stub_tenant: None,
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth(tenant_id: &str) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: TenantRateLimiter::new(1000),
            log_rate_limiter: TenantRateLimiter::new(1000),
            metric_rate_limiter: TenantRateLimiter::new(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(),
            metrics: Arc::new(observability::IngestGatewayMetrics::new()),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_rate_limit(tenant_id: &str, per_second: u32) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: TenantRateLimiter::new(per_second),
            log_rate_limiter: TenantRateLimiter::new(per_second),
            metric_rate_limiter: TenantRateLimiter::new(per_second),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(10_000),
            deployment_registry: DeploymentRegistry::new(),
            metrics: Arc::new(observability::IngestGatewayMetrics::new()),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_metric_budget(tenant_id: &str, budget: u64) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: TenantRateLimiter::new(1000),
            log_rate_limiter: TenantRateLimiter::new(1000),
            metric_rate_limiter: TenantRateLimiter::new(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::new(budget),
            deployment_registry: DeploymentRegistry::new(),
            metrics: Arc::new(observability::IngestGatewayMetrics::new()),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }

    #[cfg(test)]
    pub fn with_stub_auth_and_enforced_metric_budget(tenant_id: &str, budget: u64) -> Self {
        Self {
            auth_service_url: String::new(),
            http_client: reqwest::Client::new(),
            producer: None,
            trace_rate_limiter: TenantRateLimiter::new(1000),
            log_rate_limiter: TenantRateLimiter::new(1000),
            metric_rate_limiter: TenantRateLimiter::new(1000),
            metric_cardinality: cardinality::MetricCardinalityBudget::with_enforce(budget, true),
            deployment_registry: DeploymentRegistry::new(),
            metrics: Arc::new(observability::IngestGatewayMetrics::new()),
            stub_tenant: Some(Uuid::parse_str(tenant_id).unwrap()),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("ingest-gateway")?;

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

    let brokers = observable_config::require_env("REDPANDA_BROKERS")?;
    let topic = observable_config::require_env("INGEST_TOPIC")?;
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
    let metric_cardinality_enforce: bool = std::env::var("METRIC_CARDINALITY_ENFORCE")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let grpc_max_message_bytes: usize = std::env::var("INGEST_GRPC_MAX_MESSAGE_BYTES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(4_194_304);

    let deployment_registry = DeploymentRegistry::new();
    // Feeds the registry from admin-service's deployment.markers.v1 events
    // instead of querying Postgres directly (Phase 5 "clean ingest",
    // docs/component-decomposition.md). Runs unconditionally for the process
    // lifetime; a Redpanda outage here only means stale/empty
    // deployment_id stamping on ingested spans, not an ingest failure.
    let deployment_markers_topic = observable_config::require_env("DEPLOYMENT_MARKERS_TOPIC")?;
    let consumer_registry = deployment_registry.clone();
    let consumer_brokers = brokers.clone();
    tokio::spawn(async move {
        if let Err(e) = deployment_registry::run_consumer(
            &consumer_brokers,
            "ingest-gateway",
            &deployment_markers_topic,
            consumer_registry,
        )
        .await
        {
            tracing::error!(error = %e, "deployment marker consumer stopped");
        }
    });

    let ig_metrics = Arc::new(observability::IngestGatewayMetrics::new());

    let state = AppState {
        auth_service_url: observable_config::require_env("AUTH_SERVICE_URL")?,
        http_client: reqwest::Client::new(),
        producer: Some(producer),
        trace_rate_limiter: TenantRateLimiter::new(trace_rate_limit),
        log_rate_limiter: TenantRateLimiter::new(log_rate_limit),
        metric_rate_limiter: TenantRateLimiter::new(metric_rate_limit),
        metric_cardinality: cardinality::MetricCardinalityBudget::with_enforce(
            metric_series_budget,
            metric_cardinality_enforce,
        ),
        deployment_registry,
        metrics: ig_metrics.clone(),
        #[cfg(test)]
        stub_tenant: None,
    };

    let metrics_registry = Arc::new(ig_metrics.registry.clone());
    let grpc_state = state.clone();
    let platform_state = state.clone();
    let probe_state = readyz::IngestGatewayProbeState {
        brokers: brokers.clone(),
        metrics_registry: Some(metrics_registry),
    };
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
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn grpc_max_message_bytes_defaults_to_4mib() {
        let _guard = ENV_LOCK.lock().unwrap();
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
        let _guard = ENV_LOCK.lock().unwrap();
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
