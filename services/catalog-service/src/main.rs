use std::sync::Arc;

use catalog_service::ports::{
    ControlAssets, Credentials, Decider, Directory, Environments, Telemetry, WorkerCredentials,
};
use catalog_service::{CatalogState, DEFAULT_PORT, app};
use observable_auth::authz::HttpAccessDecider;
use observable_auth::principals::HttpPrincipalDirectory;
use sqlx::PgPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("catalog-service")?;

    let db_url = observable_config::require_catalog_database_url()?;
    let db = PgPool::connect_lazy(&db_url)?;

    let port: u16 = match std::env::var("CATALOG_SERVICE_PORT") {
        Ok(v) => v.parse()?,
        Err(_) => DEFAULT_PORT,
    };

    let auth_url = observable_config::require_env("AUTH_SERVICE_URL")?;
    let admin_url = observable_config::require_env("ADMIN_SERVICE_URL")?;
    let internal_token = observable_config::require_env("INTERNAL_SERVICE_TOKEN")?;
    if internal_token.trim().is_empty() {
        anyhow::bail!(
            "INTERNAL_SERVICE_TOKEN must be set for decide-access, lookup-principals and environment classes"
        );
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
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let state = CatalogState::new(
        db,
        Credentials::Http {
            auth_url: Arc::new(auth_url.clone()),
            client: client.clone(),
        },
        Decider::Http(HttpAccessDecider::new(
            &auth_url,
            &internal_token,
            client.clone(),
        )),
        Directory::Http(HttpPrincipalDirectory::new(
            &auth_url,
            &internal_token,
            client.clone(),
        )),
        Environments::Http {
            base_url: Arc::new(admin_url.clone()),
            internal_token: Arc::new(internal_token.clone()),
            client: client.clone(),
        },
        Telemetry::ClickHouse(Box::new(ch)),
    )
    .with_worker_credentials(WorkerCredentials::Http {
        auth_url: Arc::new(auth_url.clone()),
        internal_token: Arc::new(internal_token.clone()),
        client: client.clone(),
    })
    .with_internal_token(internal_token.clone());

    // Dashboards are pulled from control into the asset registry (phase 1 S6).
    let pull_secs = std::env::var("CATALOG_ASSET_PULL_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(60);
    tokio::spawn(catalog_service::coverage::start_control_asset_worker(
        state.clone(),
        ControlAssets::Http {
            base_url: Arc::new(admin_url),
            internal_token: Arc::new(internal_token),
            client,
        },
        std::time::Duration::from_secs(pull_secs),
    ));

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "catalog-service listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
