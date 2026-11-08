use health_service::{DEFAULT_PORT, HealthState, app};
use sqlx::PgPool;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("health-service")?;

    let db_url = observable_config::require_env("HEALTH_DATABASE_URL")?;
    let db = PgPool::connect_lazy(&db_url)?;

    let port: u16 = match std::env::var("HEALTH_SERVICE_PORT") {
        Ok(v) => v.parse()?,
        Err(_) => DEFAULT_PORT,
    };

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "health-service listening");
    axum::serve(listener, app(HealthState::new(db))).await?;
    Ok(())
}
