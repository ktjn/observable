use catalog_service::{CatalogState, DEFAULT_PORT, app};
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

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "catalog-service listening");
    axum::serve(listener, app(CatalogState::new(db))).await?;
    Ok(())
}
