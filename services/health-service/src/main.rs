use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use health_service::ports::{CatalogInputs, Credentials, Decider, Evidence, ServiceReads};
use health_service::{DEFAULT_PORT, HealthState, app};
use observable_auth::authz::HttpAccessDecider;
use sqlx::PgPool;
use uuid::Uuid;

/// Worker credentials from the secret store: a JSON object of tenant id -> `ohw_` token.
/// Without the file the worker evaluates nothing and reads disclose `evaluation_unavailable`.
fn worker_credentials() -> anyhow::Result<HashMap<Uuid, String>> {
    let Ok(path) = std::env::var("HEALTH_WORKER_CREDENTIALS_FILE") else {
        return Ok(HashMap::new());
    };
    let raw = std::fs::read_to_string(&path)?;
    let map: HashMap<Uuid, String> = serde_json::from_str(&raw).map_err(|_| {
        anyhow::anyhow!("HEALTH_WORKER_CREDENTIALS_FILE must map tenant ids to tokens")
    })?;
    if map.values().any(|t| !t.starts_with("ohw_")) {
        anyhow::bail!("HEALTH_WORKER_CREDENTIALS_FILE holds a token without the ohw_ prefix");
    }
    Ok(map)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("health-service")?;

    let db_url = observable_config::require_env("HEALTH_DATABASE_URL")?;
    let db = PgPool::connect_lazy(&db_url)?;

    let port: u16 = match std::env::var("HEALTH_SERVICE_PORT") {
        Ok(v) => v.parse()?,
        Err(_) => DEFAULT_PORT,
    };

    let auth_url = observable_config::require_env("AUTH_SERVICE_URL")?;
    let catalog_url = observable_config::require_env("CATALOG_URL")?;
    let internal_token = observable_config::require_env("INTERNAL_SERVICE_TOKEN")?;
    if internal_token.trim().is_empty() {
        anyhow::bail!("INTERNAL_SERVICE_TOKEN must be set for decide-access and catalog inputs");
    }
    let ch = clickhouse::Client::default()
        .with_url(observable_config::require_env("CLICKHOUSE_URL")?)
        .with_user(observable_config::require_env("CLICKHOUSE_USER")?)
        .with_password(observable_config::require_env_or("CLICKHOUSE_PASSWORD", ""))
        .with_database("observable")
        // clickhouse-rs 0.15 cannot decode ClickHouse 26.9 compressed framing; see
        // docs/agent-context.md.
        .with_compression(clickhouse::Compression::None);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let inputs = CatalogInputs::Http {
        base_url: Arc::new(catalog_url.clone()),
        internal_token: Arc::new(internal_token.clone()),
        workers: Arc::new(worker_credentials()?),
        client: client.clone(),
    };
    let evidence = Evidence::clickhouse(ch);
    let state = HealthState::new(db.clone()).with_ports(
        Credentials::Http {
            auth_url: Arc::new(auth_url.clone()),
            client: client.clone(),
        },
        Decider::Http(HttpAccessDecider::new(
            &auth_url,
            &internal_token,
            client.clone(),
        )),
        ServiceReads::Http {
            base_url: Arc::new(catalog_url),
            client,
        },
        inputs.clone(),
        evidence.clone(),
    );

    let interval = std::env::var("HEALTH_EVALUATION_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(60);
    tokio::spawn(health_service::assessment::start_evaluation_worker(
        db,
        inputs,
        evidence,
        Duration::from_secs(interval),
    ));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "health-service listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
