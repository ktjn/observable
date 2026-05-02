mod consumer;
mod normalise;

use domain::{EnvelopePayload, TelemetryEnvelope};
use tracing::Instrument as _;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = domain::telemetry::init_self_observability_telemetry("stream-processor")?;
    let brokers = std::env::var("REDPANDA_BROKERS")?;
    let topic = std::env::var("INGEST_TOPIC")?;
    let writer_url =
        std::env::var("STORAGE_WRITER_URL").unwrap_or_else(|_| "http://localhost:4320".into());
    let http = reqwest::Client::new();

    let qc = consumer::QueueConsumer::new(&brokers, "stream-processor", &topic)?;
    qc.run(|env: TelemetryEnvelope| {
        let http = http.clone();
        let writer_url = writer_url.clone();
        let tenant_id = env.tenant_id;
        let environment = env.environment.clone();
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
