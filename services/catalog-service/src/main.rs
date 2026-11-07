use std::sync::Arc;

use catalog_service::ports::{Credentials, Decider, Directory};
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
    let internal_token = observable_config::require_env("INTERNAL_SERVICE_TOKEN")?;
    if internal_token.trim().is_empty() {
        anyhow::bail!("INTERNAL_SERVICE_TOKEN must be set for decide-access and lookup-principals");
    }
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
            client,
        )),
    );

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "catalog-service listening");
    axum::serve(listener, app(state)).await?;
    Ok(())
}
