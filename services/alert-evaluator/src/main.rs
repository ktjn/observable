use alert_evaluator::middleware::auth::InternalServiceToken;
use alert_evaluator::{
    AppState, alerts, evaluator, incidents, internal, middleware, notifications, observability,
    readyz, slos,
};
use axum::{
    Extension, Router, middleware as axum_middleware,
    routing::{delete, get, patch, post},
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("alert-evaluator")?;

    let database_url = observable_config::require_database_url()?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let ch_url = observable_config::require_env("CLICKHOUSE_URL")?;
    let ch_user = observable_config::require_env("CLICKHOUSE_USER")?;
    let ch_password = observable_config::require_env_or("CLICKHOUSE_PASSWORD", "");
    let ch = Client::default()
        .with_url(ch_url)
        .with_user(ch_user)
        .with_password(ch_password)
        .with_database("observable")
        // clickhouse-rs 0.15.2's LZ4 response decoder is incompatible with
        // ClickHouse 26.9's compressed block framing (decompression error:
        // incorrect magic number) -- see docs/agent-context.md. Remove this
        // once a clickhouse-rs release fixes it.
        .with_compression(clickhouse::Compression::None);

    let interval_secs = std::env::var("ALERT_EVAL_INTERVAL_SECONDS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);

    let port: u16 = std::env::var("ALERT_EVALUATOR_PORT")
        .unwrap_or_else(|_| "4322".into())
        .parse()?;

    // Used by `middleware::auth::require_tenant` to validate alert-rule CRUD
    // callers against auth-service.
    let auth_service_url = observable_config::require_env("AUTH_SERVICE_URL")?;
    let http_client = reqwest::Client::new();

    // Gates the service-to-service `/internal/*` routes below.
    let internal_service_token = observable_config::require_env("INTERNAL_SERVICE_TOKEN")?;

    tokio::spawn(evaluator::start_eval_worker(
        db.clone(),
        ch.clone(),
        std::time::Duration::from_secs(interval_secs),
    ));

    tokio::spawn(evaluator::notification_worker(db.clone()));

    let metrics = Arc::new(observability::AlertEvaluatorMetrics::new());
    let state = AppState {
        db,
        ch,
        auth_service_url,
        http_client,
        metrics,
    };

    // Alert-rule CRUD (moved from admin-service, Phase 6 "consolidate alerting",
    // docs/component-decomposition.md) is tenant-authenticated. The routes are
    // declared before the require_tenant/Extension layers so health/readyz/metrics
    // below stay open.
    let app = Router::new()
        .route("/v1/admin/alerts/rules", post(alerts::handle_create_rule))
        .route(
            "/v1/admin/alerts/rules/{rule_id}/silence",
            patch(alerts::handle_silence_rule),
        )
        .route(
            "/v1/admin/alerts/rules/{rule_id}/runbook",
            patch(alerts::handle_update_rule_runbook),
        )
        .route(
            "/v1/admin/alerts/rules/{rule_id}",
            patch(alerts::handle_update_rule),
        )
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route(
            "/v1/slos",
            get(slos::handle_list_slos).post(slos::handle_create_slo),
        )
        .route(
            "/v1/notifications/channels",
            get(notifications::handle_list_channels).post(notifications::handle_create_channel),
        )
        .route(
            "/v1/notifications/channels/{id}",
            delete(notifications::handle_delete_channel),
        )
        .route("/v1/incidents", get(incidents::handle_list_incidents))
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .layer(Extension(state.db.clone()))
        .layer(Extension(Arc::new(state.auth_service_url.clone())))
        .layer(Extension(state.http_client.clone()))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/readyz", get(readyz::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Service-to-service only routes (Phase 6's internal.rs split by owner,
    // docs/component-decomposition.md) -- gated by a shared internal token
    // instead of require_tenant. Built as a separate sub-router so
    // require_internal_service never wraps the tenant-scoped routes above.
    let internal_router = Router::new()
        .route(
            "/internal/alerting-enrichment",
            get(internal::handle_alerting_enrichment),
        )
        .route(
            "/internal/alerting-correlation",
            get(internal::handle_alerting_correlation),
        )
        .layer(axum_middleware::from_fn(
            middleware::auth::require_internal_service,
        ))
        .layer(Extension(InternalServiceToken(internal_service_token)))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let app = app.merge(internal_router);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "alert-evaluator listening");
    axum::serve(listener, app).await?;
    Ok(())
}
