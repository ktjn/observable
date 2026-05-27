mod consumer;
mod metrics;

use domain::{EnvelopePayload, TelemetryEnvelope};
use std::sync::Arc;
use std::time::Duration;
use stream_processor::{
    batch,
    readyz::{StreamProcessorProbeState, readyz},
};
use tokio::time;
use tracing::Instrument as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("stream-processor")?;
    let brokers = std::env::var("REDPANDA_BROKERS")?;
    let topic = std::env::var("INGEST_TOPIC")?;
    let writer_url =
        std::env::var("STORAGE_WRITER_URL").unwrap_or_else(|_| "http://localhost:4320".into());
    let http = reqwest::Client::new();

    let max_size: usize = std::env::var("STREAM_PROCESSOR_BATCH_SIZE")
        .unwrap_or_else(|_| "500".into())
        .parse()
        .unwrap_or(500);
    let max_wait = Duration::from_millis(
        std::env::var("STREAM_PROCESSOR_BATCH_INTERVAL_MS")
            .unwrap_or_else(|_| "200".into())
            .parse()
            .unwrap_or(200),
    );

    let aggregator = Arc::new(metrics::SpanMetricsAggregator::new());

    // Spawn the probe HTTP server
    let probe_port: u16 = std::env::var("STREAM_PROCESSOR_PLATFORM_PORT")
        .unwrap_or_else(|_| "4323".into())
        .parse()?;
    let probe_state = StreamProcessorProbeState {
        brokers: brokers.clone(),
    };
    tokio::spawn(async move {
        use axum::{Router, routing::get};
        use tower_http::trace::TraceLayer;

        let app = Router::new()
            .route("/health", get(|| async { axum::http::StatusCode::OK }))
            .route("/readyz", get(readyz))
            .layer(TraceLayer::new_for_http())
            .with_state(probe_state);
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", probe_port))
            .await
            .expect("bind probe server");
        tracing::info!(port = probe_port, "stream-processor probe server listening");
        axum::serve(listener, app)
            .await
            .expect("probe server error");
    });

    // Background task to flush span metrics every 60 s
    let agg_clone = aggregator.clone();
    let http_clone = http.clone();
    let writer_url_clone = writer_url.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let (series, points) = agg_clone.flush();
            if !series.is_empty() {
                let res = http_clone
                    .post(format!("{writer_url_clone}/internal/metrics"))
                    .json(&serde_json::json!({ "series": series, "points": points }))
                    .send()
                    .await;
                if let Err(e) = res {
                    tracing::error!(error = %e, "failed to flush span metrics");
                } else {
                    tracing::info!(count = series.len(), "flushed span metrics");
                }
            }
        }
    });

    let qc = consumer::QueueConsumer::new(&brokers, "stream-processor", &topic)?;
    qc.run_batch(
        max_size,
        max_wait,
        move |envelopes: Vec<TelemetryEnvelope>| {
            let http = http.clone();
            let writer_url = writer_url.clone();
            let aggregator = aggregator.clone();

            let is_all_observable = envelopes.iter().all(|e| e.environment == "observable");
            let first_non_obs_env = envelopes
                .iter()
                .find(|e| e.environment != "observable")
                .map(|e| e.environment.clone());
            let span = if is_all_observable {
                tracing::Span::none()
            } else {
                tracing::info_span!("process_batch")
            };

            async move {
                // Record span metrics before normalisation (needs raw span values)
                for env in &envelopes {
                    if let EnvelopePayload::Spans(ref spans) = env.payload {
                        for s in spans {
                            aggregator.record_span(s, env.tenant_id);
                        }
                    }
                }

                let merged = batch::merge_batch(envelopes);

                let mut headers = reqwest::header::HeaderMap::new();
                if !is_all_observable {
                    domain::telemetry::inject_current_context(&mut headers);
                }
                let env_val = first_non_obs_env.as_deref().unwrap_or("observable");
                headers.insert(
                    "x-observable-environment",
                    env_val
                        .parse()
                        .unwrap_or_else(|_| "unknown".parse().unwrap()),
                );

                if !merged.spans.is_empty() {
                    http.post(format!("{writer_url}/internal/spans"))
                        .headers(headers.clone())
                        .json(&merged.spans)
                        .send()
                        .await?;
                }
                if !merged.logs.is_empty() {
                    http.post(format!("{writer_url}/internal/logs"))
                        .headers(headers.clone())
                        .json(&merged.logs)
                        .send()
                        .await?;
                }
                if !merged.series.is_empty() || !merged.points.is_empty() {
                    http.post(format!("{writer_url}/internal/metrics"))
                        .headers(headers)
                        .json(
                            &serde_json::json!({ "series": merged.series, "points": merged.points }),
                        )
                        .send()
                        .await?;
                }
                Ok(())
            }
            .instrument(span)
        },
    )
    .await
}
