use admin_service::{
    AdminServiceAppState, admin_members, config, middleware, observability, tokens,
};
use axum::{
    Router,
    http::StatusCode,
    middleware as axum_middleware,
    routing::{delete, get, post},
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("admin-service")?;

    let ch_url = std::env::var("CLICKHOUSE_URL").unwrap_or_else(|_| "http://localhost:8123".into());
    let ch_user = std::env::var("CLICKHOUSE_USER").unwrap_or_else(|_| "default".into());
    let ch_password = std::env::var("CLICKHOUSE_PASSWORD").unwrap_or_default();
    let ch = Client::default()
        .with_url(ch_url)
        .with_user(ch_user)
        .with_password(ch_password)
        .with_database("observable");

    let database_url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/observable".into());
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let port: u16 = std::env::var("ADMIN_SERVICE_PORT")
        .unwrap_or_else(|_| "4324".into())
        .parse()?;

    let auth_service_url =
        std::env::var("AUTH_SERVICE_URL").unwrap_or_else(|_| "http://auth-service:4319".into());

    let state = AdminServiceAppState {
        db,
        ch,
        auth_service_url,
        metrics: Arc::new(observability::AdminServiceMetrics::new()),
    };

    let app = Router::new()
        .route("/v1/admin/members", get(admin_members::handle_list_members))
        .route("/v1/admin/members", post(admin_members::handle_add_member))
        .route(
            "/v1/admin/members/{user_id}/role",
            axum::routing::put(admin_members::handle_update_role),
        )
        .route(
            "/v1/admin/members/{user_id}",
            delete(admin_members::handle_remove_member),
        )
        .route(
            "/v1/admin/members/{user_id}/revoke-sessions",
            post(admin_members::handle_revoke_sessions),
        )
        .route("/v1/tokens", get(tokens::list_tokens))
        .route("/v1/tokens", post(tokens::create_token))
        .route("/v1/tokens/{id}", delete(tokens::revoke_token))
        .route("/v1/tokens/{id}/renew", post(tokens::renew_token))
        .route("/v1/tokens/{id}/restore", post(tokens::restore_token))
        .route("/v1/tokens/{id}/permanent", delete(tokens::delete_token))
        .route("/v1/config", get(config::get_config))
        .route("/v1/config/llm", axum::routing::put(config::put_llm_config))
        .route("/v1/config/llm/models", post(config::list_llm_models))
        .route(
            "/v1/config/llm-key",
            axum::routing::put(config::put_llm_key),
        )
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .layer(axum::Extension(state.db.clone()))
        .layer(axum::Extension(Arc::new(state.auth_service_url.clone())))
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(domain::telemetry::OtelMakeSpan::new(Level::INFO)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "admin-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
