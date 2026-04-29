mod alerts;
mod audit;
mod dashboards;
mod deployments;
mod discovery;
mod logs;
mod mcp_tools;
mod metrics;
mod middleware;
mod planner;
mod schemas;
mod sql_templates;
mod traces;

use axum::{
    middleware as axum_middleware,
    routing::{get, patch, post},
    Router,
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    domain::telemetry::init_self_observability_telemetry("query-api")?;
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
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(planner::QueryPlanner),
    };
    let app = Router::new()
        .route("/v1/traces", get(traces::search_traces))
        .route("/v1/traces/:trace_id", get(traces::get_trace))
        .route("/v1/logs", get(logs::search_logs))
        .route("/v1/logs/tail", get(logs::tail_logs))
        .route("/v1/logs/:log_id/context", get(logs::get_log_context))
        .route("/v1/metrics", get(metrics::list_metrics))
        .route("/v1/metrics/:series_id", get(metrics::get_metric_points))
        .route("/v1/topology", get(discovery::get_topology))
        .route(
            "/v1/infrastructure",
            get(discovery::list_infrastructure_inventory),
        )
        .route(
            "/v1/infrastructure/:entity_type/:entity_id",
            get(discovery::get_infrastructure_detail),
        )
        .route("/v1/services", get(discovery::list_services))
        .route(
            "/v1/services/summary",
            get(discovery::list_service_summaries),
        )
        .route(
            "/v1/services/:service_name/summary",
            get(discovery::get_service_summary),
        )
        .route("/v1/environments", get(discovery::list_environments))
        .route("/v1/deployments", get(deployments::list_deployments))
        .route("/v1/dashboards", get(dashboards::handle_list_dashboards))
        .route("/v1/dashboards", post(dashboards::handle_create_dashboard))
        .route("/v1/alerts/rules", get(alerts::handle_list_rules))
        .route("/v1/alerts/rules", post(alerts::handle_create_rule))
        .route(
            "/v1/alerts/rules/:rule_id/silence",
            patch(alerts::handle_silence_rule),
        )
        .route(
            "/v1/schemas/:signal_type/attributes",
            get(schemas::handle_list_attributes),
        )
        .route(
            "/v1/schemas/:signal_type/attributes/:key/annotations",
            get(schemas::handle_get_annotation)
                .put(schemas::handle_upsert_annotation)
                .patch(schemas::handle_patch_annotation)
                .delete(schemas::handle_delete_annotation),
        )
        .route(
            "/v1/mcp/tools/metric-schema/:metric_name",
            get(mcp_tools::handle_get_metric_schema),
        )
        .route(
            "/v1/mcp/tools/signal-fields/:signal_type",
            get(mcp_tools::handle_list_signal_fields),
        )
        .route(
            "/v1/mcp/tools/resolve-label/:signal_type",
            get(mcp_tools::handle_resolve_label),
        )
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .route("/health", get(|| async { axum::http::StatusCode::OK }))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "query-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}
