mod consumer;
mod normalise;

use domain::{EnvelopePayload, TelemetryEnvelope};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let otlp = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
    domain::telemetry::init_telemetry("stream-processor", otlp.as_deref())?;
    let brokers = std::env::var("REDPANDA_BROKERS")?;
    let topic = std::env::var("INGEST_TOPIC")?;
    let writer_url =
        std::env::var("STORAGE_WRITER_URL").unwrap_or_else(|_| "http://localhost:4320".into());
    let http = reqwest::Client::new();

    let qc = consumer::QueueConsumer::new(&brokers, "stream-processor", &topic)?;
    qc.run(|env: TelemetryEnvelope| {
        let http = http.clone();
        let writer_url = writer_url.clone();
        async move {
            let tenant_id = env.tenant_id;
            match env.payload {
                EnvelopePayload::Spans(spans) => {
                    let normalised: Vec<_> = spans
                        .into_iter()
                        .map(|s| normalise::normalise_span(s, tenant_id))
                        .collect();
                    http.post(format!("{writer_url}/internal/spans"))
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
                        .json(&serde_json::json!({ "series": series, "points": points }))
                        .send()
                        .await?;
                }
            }
            Ok(())
        }
    })
    .await
}
