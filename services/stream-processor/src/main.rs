mod metrics;
mod normalized_producer;

use domain::{EnvelopePayload, NormalizedTelemetryBatch, TelemetryEnvelope};
use normalized_producer::NormalizedProducer;
use std::sync::Arc;
use std::time::Duration;
use stream_processor::{
    batch, consumer, observability,
    readyz::{StreamProcessorProbeState, readyz},
};
use tokio::time;
use tracing::Instrument as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = observable_telemetry::init_self_observability_telemetry("stream-processor")?;
    let brokers = observable_config::require_env("REDPANDA_BROKERS")?;
    let topic = observable_config::require_env("INGEST_TOPIC")?;
    let normalized_topic = observable_config::require_env("NORMALIZED_TOPIC")?;
    let normalized_producer = Arc::new(NormalizedProducer::new(&brokers, &normalized_topic)?);

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
    let sp_metrics = observability::StreamProcessorMetrics::new();
    let metrics_registry = Arc::new(sp_metrics.registry.clone());
    let probe_state = StreamProcessorProbeState {
        brokers: brokers.clone(),
        metrics_registry: Some(metrics_registry),
    };
    tokio::spawn(async move {
        use axum::{Router, routing::get};
        use tower_http::trace::TraceLayer;

        let app = Router::new()
            .route("/health", get(|| async { axum::http::StatusCode::OK }))
            .route("/readyz", get(readyz))
            .route("/metrics", get(observability::metrics))
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

    // Background task to flush span-derived metrics every 60 s onto
    // `telemetry.normalized.v1` (ADR-029 / ADR-035).
    let agg_clone = aggregator.clone();
    let producer_clone = normalized_producer.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let (series, points) = agg_clone.flush();
            if !series.is_empty() || !points.is_empty() {
                let count = series.len();
                let batch = NormalizedTelemetryBatch {
                    spans: Vec::new(),
                    logs: Vec::new(),
                    series,
                    points,
                };
                let key = uuid::Uuid::new_v4().to_string();
                if let Err(e) = producer_clone.publish(&batch, &key).await {
                    tracing::error!(error = %e, "failed to publish span metrics to normalized topic");
                } else {
                    tracing::info!(count, "published span metrics to normalized topic");
                }
            }
        }
    });

    let qc = consumer::QueueConsumer::new(&brokers, "stream-processor", &topic)?;
    qc.run_batch(
        max_size,
        max_wait,
        move |envelopes: Vec<TelemetryEnvelope>| {
            let aggregator = aggregator.clone();
            let normalized_producer = normalized_producer.clone();

            // Suppresses span creation for self-observability data to avoid a
            // feedback loop (this service's own traces would otherwise
            // generate more traces through this same pipeline).
            let is_all_observable = envelopes
                .iter()
                .all(|e| observable_telemetry::is_self_telemetry_env(&e.environment));
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

                if !merged.spans.is_empty()
                    || !merged.logs.is_empty()
                    || !merged.series.is_empty()
                    || !merged.points.is_empty()
                {
                    let normalized = NormalizedTelemetryBatch {
                        spans: merged.spans,
                        logs: merged.logs,
                        series: merged.series,
                        points: merged.points,
                    };
                    let key = uuid::Uuid::new_v4().to_string();
                    normalized_producer.publish(&normalized, &key).await?;
                }
                Ok(())
            }
            .instrument(span)
        },
    )
    .await
}
