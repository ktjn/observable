mod alerts;
mod audit;
mod change_events;
mod dashboards;
mod deployments;
mod discovery;
mod incidents;
mod llm_adapter;
mod llm_config;
mod logs;
mod mcp_query;
mod mcp_tools;
mod metrics;
mod middleware;
mod notifications;
mod observability;
mod planner;
mod reliability;
mod saved_views;
mod schemas;
mod setup;
mod slos;
mod sql_templates;
mod tenants;
mod traces;

use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post, put},
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("query-api")?;
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
    let port: u16 = std::env::var("QUERY_API_PORT")
        .unwrap_or_else(|_| "8090".into())
        .parse()?;
    let llm: Option<Arc<dyn llm_adapter::LlmCaller>> = llm_adapter::OpenAiLlmCaller::from_env()
        .map(|c| Arc::new(c) as Arc<dyn llm_adapter::LlmCaller>);
    if llm.is_none() {
        tracing::info!(
            "LLM_API_KEY env var not set — NLQ will resolve config from DB at call time \
             (supports Ollama and other no-auth providers)"
        );
    }
    let auth_service_url =
        std::env::var("AUTH_SERVICE_URL").unwrap_or_else(|_| "http://auth-service:4319".into());
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(planner::QueryPlanner),
        llm,
        auth_service_url,
        metrics: Arc::new(observability::QueryApiMetrics::new()),
    };
    let app = Router::new()
        .route("/v1/traces", get(traces::search_traces))
        .route("/v1/traces/histogram", get(traces::trace_histogram))
        .route("/v1/traces/{trace_id}", get(traces::get_trace))
        .route("/v1/logs", get(logs::search_logs))
        .route("/v1/logs/histogram", get(logs::log_histogram))
        .route("/v1/logs/tail", get(logs::tail_logs))
        .route("/v1/logs/{log_id}/context", get(logs::get_log_context))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/points", get(metrics::get_metric_group_points))
        .route("/v1/metrics/{series_id}", get(metrics::get_metric_points))
        .route("/v1/setup/status", get(setup::get_setup_status))
        .route("/v1/topology", get(discovery::get_topology))
        .route(
            "/v1/infrastructure",
            get(discovery::list_infrastructure_inventory),
        )
        .route(
            "/v1/infrastructure/{entity_type}/{entity_id}",
            get(discovery::get_infrastructure_detail),
        )
        .route("/v1/services", get(discovery::list_services))
        .route(
            "/v1/services/summary",
            get(discovery::list_service_summaries),
        )
        .route(
            "/v1/services/{service_name}/summary",
            get(discovery::get_service_summary),
        )
        .route(
            "/v1/services/{service_name}/response-time-history",
            get(discovery::get_service_response_time_history),
        )
        .route("/v1/environments", get(discovery::list_environments))
        .route("/v1/deployments", get(deployments::list_deployments))
        .route(
            "/v1/events/changes",
            get(change_events::handle_list_change_events),
        )
        .route("/v1/dashboards", get(dashboards::handle_list_dashboards))
        .route("/v1/dashboards", post(dashboards::handle_create_dashboard))
        .route("/v1/dashboards/{id}", get(dashboards::handle_get_dashboard))
        .route(
            "/v1/dashboards/{id}",
            put(dashboards::handle_update_dashboard).delete(dashboards::handle_delete_dashboard),
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
            axum::routing::delete(dashboards::handle_revoke_grant),
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
            axum::routing::delete(saved_views::handle_revoke_saved_view_grant),
        )
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules/{rule_id}", get(alerts::handle_get_rule))
        .route("/v1/incidents", get(incidents::handle_list_incidents))
        .route(
            "/v1/incidents/{incident_id}",
            get(incidents::handle_get_incident),
        )
        .route(
            "/v1/services/{service_name}/reliability-report",
            get(reliability::handle_get_service_reliability_report),
        )
        .route(
            "/v1/notifications/channels",
            get(notifications::handle_list_channels),
        )
        .route(
            "/v1/notifications/channels",
            post(notifications::handle_create_channel),
        )
        .route(
            "/v1/notifications/channels/{id}",
            delete(notifications::handle_delete_channel),
        )
        .route("/v1/slos", get(slos::handle_list_slos))
        .route("/v1/slos", post(slos::handle_create_slo))
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
            "/v1/mcp/tools/metric-schema/{metric_name}",
            get(mcp_tools::handle_get_metric_schema),
        )
        .route(
            "/v1/mcp/tools/signal-fields/{signal_type}",
            get(mcp_tools::handle_list_signal_fields),
        )
        .route(
            "/v1/mcp/tools/resolve-label/{signal_type}",
            get(mcp_tools::handle_resolve_label),
        )
        .route("/v1/mcp/query", post(mcp_query::handle_mcp_query))
        .route("/v1/nlq", post(llm_adapter::handle_nlq_query))
        .route("/v1/nlq/metadata", get(llm_adapter::handle_nlq_metadata))
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .layer(axum::Extension(state.db.clone()))
        .layer(axum::Extension(Arc::new(state.auth_service_url.clone())))
        // Bootstrap endpoints — no tenant-auth required; used to populate the
        // global tenant+environment selector before a scope is chosen.
        .route("/v1/tenants", get(tenants::list_tenants))
        .route(
            "/v1/tenants/{id}/environments",
            get(tenants::list_tenant_environments),
        )
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .route("/readyz", get(observability::readyz))
        .route("/metrics", get(observability::metrics))
        .layer(axum_middleware::from_fn_with_state(
            state.clone(),
            observability::record_http_metrics,
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "query-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}
