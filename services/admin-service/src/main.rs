use admin_service::{
    AdminServiceAppState, admin_members, alerts, change_events, config, dashboards, deployments,
    incidents, middleware, notifications, observability, saved_views, schemas, slos, tenants,
    tokens, usage,
};
use axum::{
    Router,
    http::StatusCode,
    middleware as axum_middleware,
    routing::{delete, get, patch, post},
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("admin-service")?;

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

    let database_url = observable_config::require_database_url()?;
    let db = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let port: u16 = std::env::var("ADMIN_SERVICE_PORT")
        .unwrap_or_else(|_| "4324".into())
        .parse()?;

    let auth_service_url = observable_config::require_env("AUTH_SERVICE_URL")?;
    let http_client = reqwest::Client::new();

    let state = AdminServiceAppState {
        db,
        ch,
        auth_service_url,
        http_client: http_client.clone(),
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
        .route(
            "/v1/tenants/usage-report",
            get(usage::handle_get_tenant_usage_report),
        )
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
            "/v1/deployments",
            get(deployments::list_deployments).post(deployments::create_deployment),
        )
        .route(
            "/v1/deployments/{deployment_id}",
            patch(deployments::finish_deployment),
        )
        .route(
            "/v1/events/changes",
            get(change_events::handle_list_change_events).post(change_events::create_change_event),
        )
        .route(
            "/v1/schemas/{signal_type}/attributes",
            get(schemas::handle_list_attributes),
        )
        .route(
            "/v1/schemas/{signal_type}/attributes/{key}/annotations",
            get(schemas::handle_get_annotation)
                .put(schemas::handle_upsert_annotation)
                .patch(schemas::handle_patch_annotation)
                .delete(schemas::handle_delete_annotation),
        )
        .route(
            "/v1/saved-views",
            get(saved_views::handle_list_saved_views).post(saved_views::handle_create_saved_view),
        )
        .route(
            "/v1/saved-views/{id}",
            get(saved_views::handle_get_saved_view)
                .put(saved_views::handle_update_saved_view)
                .delete(saved_views::handle_delete_saved_view),
        )
        .route(
            "/v1/saved-views/{id}/grants",
            get(saved_views::handle_list_saved_view_grants)
                .post(saved_views::handle_add_saved_view_grant),
        )
        .route(
            "/v1/saved-views/{id}/grants/{user_id}",
            delete(saved_views::handle_revoke_saved_view_grant),
        )
        .route(
            "/v1/dashboards",
            get(dashboards::handle_list_dashboards).post(dashboards::handle_create_dashboard),
        )
        .route(
            "/v1/dashboards/{id}",
            get(dashboards::handle_get_dashboard)
                .put(dashboards::handle_update_dashboard)
                .delete(dashboards::handle_delete_dashboard),
        )
        .route(
            "/v1/dashboards/import",
            post(dashboards::handle_import_dashboard),
        )
        .route(
            "/v1/dashboards/{id}/export",
            get(dashboards::handle_get_dashboard_export),
        )
        .route(
            "/v1/dashboards/{id}/grants",
            get(dashboards::handle_list_grants).post(dashboards::handle_add_grant),
        )
        .route(
            "/v1/dashboards/{id}/grants/{user_id}",
            delete(dashboards::handle_revoke_grant),
        )
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
        .layer(axum::Extension(state.db.clone()))
        .layer(axum::Extension(Arc::new(state.auth_service_url.clone())))
        .layer(axum::Extension(http_client))
        // Bootstrap endpoints — no tenant-auth required; used to populate the
        // global tenant+environment selector before a scope is chosen. Must stay
        // outside the require_tenant layer above.
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .route("/health", get(|| async { StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(observable_telemetry::OtelMakeSpan::new(Level::INFO)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "admin-service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
