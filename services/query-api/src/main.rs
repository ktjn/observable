mod audit;
mod discovery;
mod incidents;
mod llm_adapter;
mod llm_config;
mod logs;
mod mcp_query;
mod mcp_tools;
mod metrics;
mod middleware;
mod nlq_session;
mod notifications;
mod observability;
mod planner;
mod reliability;
mod setup;
mod sql_templates;
mod traces;

use axum::{
    Router, middleware as axum_middleware,
    routing::{delete, get, post},
};
use clickhouse::Client;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("query-api")?;
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
    let auth_service_url = observable_config::require_env("AUTH_SERVICE_URL")?;
    let state = traces::AppState {
        ch,
        db,
        planner: Arc::new(planner::QueryPlanner),
        llm,
        auth_service_url,
        metrics: Arc::new(observability::QueryApiMetrics::new()),
        sessions: nlq_session::NlqSessionStore::default(),
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
        .route("/v1/nlq/prepare", post(llm_adapter::handle_nlq_prepare))
        .route("/v1/nlq/complete", post(llm_adapter::handle_nlq_complete))
        .route("/v1/nlq/metadata", get(llm_adapter::handle_nlq_metadata))
        .layer(axum_middleware::from_fn(middleware::auth::require_tenant))
        .layer(axum::Extension(state.db.clone()))
        .layer(axum::Extension(Arc::new(state.auth_service_url.clone())))
        .layer(axum::Extension(reqwest::Client::new()))
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
