mod dev_bootstrap;

use auth_service::{
    observability,
    oidc::{OidcConfig, OidcState},
};
use axum::{
    Router,
    http::StatusCode,
    routing::{get, post},
};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("auth-service")?;

    let db_url = observable_config::require_auth_database_url()?;
    let db = PgPool::connect(&db_url).await?;

    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4319".into())
        .parse()?;

    let dev_mode = std::env::var("OBSERVABLE_ENV").as_deref() == Ok("dev");
    let session_secret =
        auth_service::resolve_session_secret(std::env::var("SESSION_SECRET").ok(), dev_mode)?;

    let oidc_config = OidcConfig {
        issuer: observable_config::require_env("ZITADEL_ISSUER")?,
        api_base: observable_config::require_env("ZITADEL_API_BASE")?,
        client_id: observable_config::require_env("ZITADEL_CLIENT_ID")?,
        redirect_uri: observable_config::require_env("ZITADEL_REDIRECT_URI")?,
        session_secret,
        dev_mode,
    };

    let state = OidcState {
        db: db.clone(),
        config: oidc_config,
        http_client: reqwest::Client::new(),
        metrics: Arc::new(observability::AuthServiceMetrics::new()),
    };

    if dev_mode {
        let dev_email =
            std::env::var("DEV_ADMIN_EMAIL").unwrap_or_else(|_| "admin@dev.observable".into());
        if let Err(e) = dev_bootstrap::seed_dev_admin_role(&db, &dev_email).await {
            tracing::warn!(error = %e, "dev bootstrap role seed failed (non-fatal)");
        }
    }

    let internal_token = auth_service::authz::resolve_internal_token(
        std::env::var("INTERNAL_SERVICE_TOKEN").ok(),
        dev_mode,
    )?;
    let authz_state = auth_service::authz::AuthzState {
        db: db.clone(),
        internal_token: Arc::new(internal_token),
    };

    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .route("/internal/validate", post(auth_service::validate_handler))
        .route(
            "/internal/validate-session",
            post(auth_service::oidc::validate_session_handler),
        )
        .route("/v1/auth/login", get(auth_service::oidc::login_handler))
        .route(
            "/v1/auth/callback",
            get(auth_service::oidc::callback_handler),
        )
        .route("/v1/auth/logout", post(auth_service::oidc::logout_handler))
        .route("/v1/auth/me", get(auth_service::oidc::me_handler))
        .route(
            "/v1/auth/switch-tenant",
            post(auth_service::oidc::switch_tenant_handler),
        )
        .merge(auth_service::tenant_admin::router())
        .with_state(state.clone())
        .merge(auth_service::authz::router(authz_state))
        .layer(axum::middleware::from_fn_with_state(
            state,
            observability::record_http_metrics,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(observable_telemetry::OtelMakeSpan::new(Level::INFO)),
        );

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "auth-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
