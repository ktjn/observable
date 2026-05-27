mod consumer;
mod metrics;
use stream_processor::normalise;

use domain::{EnvelopePayload, TelemetryEnvelope};
use std::sync::Arc;
use stream_processor::readyz::{StreamProcessorProbeState, readyz};
use tokio::time::{self, Duration};
use tracing::Instrument as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("stream-processor")?;
    let brokers = std::env::var("REDPANDA_BROKERS")?;
    let topic = std::env::var("INGEST_TOPIC")?;
    let writer_url =
        std::env::var("STORAGE_WRITER_URL").unwrap_or_else(|_| "http://localhost:4320".into());
    let http = reqwest::Client::new();

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

    // Background task to flush metrics
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
    qc.run(|env: TelemetryEnvelope| {
        let http = http.clone();
        let writer_url = writer_url.clone();
        let tenant_id = env.tenant_id;
        let environment = env.environment.clone();
        let aggregator = aggregator.clone();
        let is_observable = environment == "observable";
        async move {
            // Do not instrument processing of observable-environment data — those
            // signals go through this same pipeline and would create a feedback loop.
            let mut headers = reqwest::header::HeaderMap::new();
            if !is_observable {
                domain::telemetry::inject_current_context(&mut headers);
            }
            headers.insert(
                "x-observable-environment",
                environment
                    .parse()
                    .unwrap_or_else(|_| "unknown".parse().unwrap()),
            );
            match env.payload {
                EnvelopePayload::Spans(spans) => {
                    for span in &spans {
                        aggregator.record_span(span, tenant_id);
                    }
                    let normalised: Vec<_> = spans
                        .into_iter()
                        .map(|s| normalise::normalise_span(s, tenant_id))
                        .collect();
                    http.post(format!("{writer_url}/internal/spans"))
                        .headers(headers)
                        .json(&normalised)
                        .send()
                        .await?;
                }
                EnvelopePayload::Logs(logs) => {
                    let normalised: Vec<_> = logs
                        .into_iter()
                        .map(|l| normalise::normalise_log(l, tenant_id))
                        .collect();
                    http.post(format!("{writer_url}/internal/logs"))
                        .headers(headers)
                        .json(&normalised)
                        .send()
                        .await?;
                }
                EnvelopePayload::Metrics { series, points } => {
                    let series: Vec<_> = series
                        .into_iter()
                        .map(|s| normalise::normalise_metric_series(s, tenant_id))
                        .collect();
                    let points: Vec<_> = points
                        .into_iter()
                        .map(|p| normalise::normalise_metric_point(p, tenant_id))
                        .collect();
                    http.post(format!("{writer_url}/internal/metrics"))
                        .headers(headers)
                        .json(&serde_json::json!({ "series": series, "points": points }))
                        .send()
                        .await?;
                }
            }
            Ok(())
        }
        .instrument(if is_observable {
            tracing::Span::none()
        } else {
            tracing::info_span!("process_envelope", %tenant_id)
        })
    })
    .await
}
