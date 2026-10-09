use axum::{extract::State, http::StatusCode};
use std::sync::Arc;

#[derive(Clone)]
pub struct IngestGatewayProbeState {
    pub brokers: String,
    pub metrics_registry: Option<Arc<prometheus::Registry>>,
}

/// Check Redpanda broker connectivity by fetching cluster metadata.
/// Ingest's only hard runtime dependency besides the auth API is the queue it
/// publishes accepted telemetry to (ADR-035 / Phase 5 "clean ingest",
/// docs/component-decomposition.md), so readiness reflects the broker rather
/// than PostgreSQL. Auth is deliberately not probed: it is a per-request
/// dependency, and a transient auth outage should surface as request failures
/// instead of pulling every ingest pod out of rotation. Mirrors
/// `stream-processor`'s readiness check; uses `spawn_blocking` because
/// rdkafka's `fetch_metadata` is synchronous.
pub async fn readyz(State(state): State<IngestGatewayProbeState>) -> StatusCode {
    let brokers = state.brokers.clone();
    let result = tokio::task::spawn_blocking(move || {
        use rdkafka::{
            ClientConfig,
            consumer::{BaseConsumer, Consumer},
        };
        let checker: BaseConsumer = ClientConfig::new()
            .set("bootstrap.servers", &brokers)
            .create()
            .map_err(|e| format!("create consumer: {e}"))?;
        checker
            .fetch_metadata(None, std::time::Duration::from_secs(2))
            .map(|_| ())
            .map_err(|e| format!("fetch metadata: {e}"))
    })
    .await;

    match result {
        Ok(Ok(())) => StatusCode::OK,
        Ok(Err(e)) => {
            tracing::warn!(error = %e, "ingest-gateway readiness redpanda check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
        Err(e) => {
            tracing::warn!(error = %e, "ingest-gateway readiness check task panicked");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
