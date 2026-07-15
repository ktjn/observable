mod dev_bootstrap;

use auth_service::{
    audit, lookup_api_key, observability,
    oidc::{OidcConfig, OidcState},
    validate,
};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::{Instrument as _, Level};
use uuid::Uuid;

#[derive(Deserialize)]
struct ValidateRequest {
    api_key: String,
}

#[derive(Serialize)]
struct ValidateResponse {
    tenant_id: Uuid,
    role: String,
    environment: String,
}

async fn validate_handler(
    State(state): State<OidcState>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, StatusCode> {
    let hash = validate::sha256_hex(&req.api_key);
    async move {
        match lookup_api_key(&state.db, &req.api_key).await {
            Ok((tenant_id, role, environment)) => {
                audit::write(&state.db, &audit::AuditEntry::allow(hash, tenant_id)).await;
                Ok(Json(ValidateResponse {
                    tenant_id,
                    role,
                    environment,
                }))
            }
            Err(e) => {
                let reason = if e.to_string().contains("revoked") {
                    "revoked"
                } else if e.to_string().contains("not found") {
                    audit::write(&state.db, &audit::AuditEntry::deny_not_found(hash)).await;
                    return Err(StatusCode::UNAUTHORIZED);
                } else {
                    "hash_mismatch"
                };
                audit::write(
                    &state.db,
                    &audit::AuditEntry::deny(hash, Uuid::nil(), reason),
                )
                .await;
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
    .instrument(tracing::info_span!("auth.validate"))
    .await
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("auth-service")?;

    let db_url = domain::config::require_env("DATABASE_URL")?;
    let db = PgPool::connect(&db_url).await?;

    let port: u16 = std::env::var("AUTH_SERVICE_PORT")
        .unwrap_or_else(|_| "4319".into())
        .parse()?;

    let dev_mode = std::env::var("OBSERVABLE_ENV").as_deref() == Ok("dev");
    let session_secret =
        auth_service::resolve_session_secret(std::env::var("SESSION_SECRET").ok(), dev_mode)?;

    let oidc_config = OidcConfig {
        issuer: domain::config::require_env("ZITADEL_ISSUER")?,
        api_base: domain::config::require_env("ZITADEL_API_BASE")?,
        client_id: domain::config::require_env("ZITADEL_CLIENT_ID")?,
        redirect_uri: domain::config::require_env("ZITADEL_REDIRECT_URI")?,
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

    let app = Router::new()
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .route("/internal/validate", post(validate_handler))
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
    tracing::info!(port, "auth-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
