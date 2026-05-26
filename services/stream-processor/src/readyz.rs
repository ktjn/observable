use axum::{extract::State, http::StatusCode};

#[derive(Clone)]
pub struct StreamProcessorProbeState {
    pub brokers: String,
}

/// Check Redpanda broker connectivity by fetching cluster metadata.
/// Uses spawn_blocking because rdkafka's fetch_metadata is synchronous.
pub async fn readyz(State(state): State<StreamProcessorProbeState>) -> StatusCode {
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
            tracing::warn!(error = %e, "stream-processor readiness redpanda check failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
        Err(e) => {
            tracing::warn!(error = %e, "stream-processor readiness check task panicked");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
