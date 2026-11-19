//! PUSH-016 (`publish-telemetry-readonly-changes`): coalesces a durably flushed
//! ClickHouse batch of spans, logs, or metric points into at most one
//! `TelemetryWatermarkEvent` (`telemetry.watermarks.v1`) per `(tenant_id, signal)`
//! and publishes it best-effort to Redpanda/Kafka only after the ClickHouse
//! insert succeeds.
//!
//! Coalescing at the flush boundary guarantees that a burst of raw telemetry
//! records produces a single visibility-watermark hint per tenant and signal
//! rather than per-record events, keeping downstream subscription refreshes
//! bounded by query count rather than record count (PUSH-016-C1/C2).

use domain::{LogRecord, MetricPoint, MetricSeries, Span, TelemetryWatermarkEvent};
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use uuid::Uuid;

/// Upper bound on distinct service names carried in a single watermark hint so
/// payload size remains bounded regardless of tenant fleet cardinality.
pub const MAX_SERVICES_PER_WATERMARK: usize = 64;

pub trait WatermarkPublisher: Send + Sync {
    fn publish<'a>(
        &'a self,
        event: &'a TelemetryWatermarkEvent,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;
}

pub struct TelemetryWatermarkProducer {
    producer: FutureProducer,
    topic: String,
}

impl TelemetryWatermarkProducer {
    pub fn new(brokers: &str, topic: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;
        Ok(Self {
            producer,
            topic: topic.to_string(),
        })
    }
}

impl WatermarkPublisher for TelemetryWatermarkProducer {
    fn publish<'a>(
        &'a self,
        event: &'a TelemetryWatermarkEvent,
    ) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let payload = serde_json::to_vec(event)?;
            let key = event.tenant_id.to_string();
            self.producer
                .send(
                    FutureRecord::to(&self.topic).key(&key).payload(&payload),
                    Duration::from_secs(5),
                )
                .await
                .map_err(|(e, _)| anyhow::anyhow!("kafka watermark produce failed: {e}"))?;
            Ok(())
        })
    }
}

pub async fn publish_watermarks_best_effort(
    publisher: Option<&dyn WatermarkPublisher>,
    events: Vec<TelemetryWatermarkEvent>,
) {
    let Some(publisher) = publisher else {
        return;
    };
    for event in &events {
        if let Err(e) = publisher.publish(event).await {
            tracing::warn!(
                error = %e,
                tenant_id = %event.tenant_id,
                signal = %event.signal,
                "failed to publish telemetry visibility watermark; continuing"
            );
        }
    }
}

#[derive(Default)]
struct TenantAccumulator {
    services: BTreeSet<String>,
    watermark_unix_nano: u64,
    coalesced_records: u64,
}

impl TenantAccumulator {
    fn record(&mut self, service_name: &str, timestamp_unix_nano: u64) {
        self.coalesced_records += 1;
        if timestamp_unix_nano > self.watermark_unix_nano {
            self.watermark_unix_nano = timestamp_unix_nano;
        }
        let trimmed = service_name.trim();
        if !trimmed.is_empty() && self.services.len() < MAX_SERVICES_PER_WATERMARK {
            self.services.insert(trimmed.to_string());
        }
    }

    fn into_event(self, tenant_id: Uuid, signal: &str) -> TelemetryWatermarkEvent {
        TelemetryWatermarkEvent {
            tenant_id,
            signal: signal.to_string(),
            services: self.services.into_iter().collect(),
            watermark_unix_nano: self.watermark_unix_nano,
            coalesced_records: self.coalesced_records,
        }
    }
}

/// Coalesces a flushed batch of spans into one `TelemetryWatermarkEvent` per tenant.
pub fn coalesce_span_watermarks(spans: &[Span]) -> Vec<TelemetryWatermarkEvent> {
    let mut by_tenant: BTreeMap<Uuid, TenantAccumulator> = BTreeMap::new();
    for span in spans {
        let ts = span.end_time_unix_nano.max(span.start_time_unix_nano);
        by_tenant
            .entry(span.tenant_id)
            .or_default()
            .record(&span.service_name, ts);
    }
    by_tenant
        .into_iter()
        .map(|(tenant_id, acc)| acc.into_event(tenant_id, "spans"))
        .collect()
}

/// Coalesces a flushed batch of log records into one `TelemetryWatermarkEvent` per tenant.
pub fn coalesce_log_watermarks(logs: &[LogRecord]) -> Vec<TelemetryWatermarkEvent> {
    let mut by_tenant: BTreeMap<Uuid, TenantAccumulator> = BTreeMap::new();
    for log in logs {
        by_tenant
            .entry(log.tenant_id)
            .or_default()
            .record(&log.service_name, log.timestamp_unix_nano);
    }
    by_tenant
        .into_iter()
        .map(|(tenant_id, acc)| acc.into_event(tenant_id, "logs"))
        .collect()
}

/// Coalesces a flushed batch of metric series and points into one
/// `TelemetryWatermarkEvent` per tenant.
pub fn coalesce_metric_watermarks(
    series: &[MetricSeries],
    points: &[MetricPoint],
) -> Vec<TelemetryWatermarkEvent> {
    let mut by_tenant: BTreeMap<Uuid, TenantAccumulator> = BTreeMap::new();
    for point in points {
        by_tenant
            .entry(point.tenant_id)
            .or_default()
            .record("", point.time_unix_nano);
    }
    for s in series {
        if let Some(acc) = by_tenant.get_mut(&s.tenant_id) {
            let trimmed = s.service_name.trim();
            if !trimmed.is_empty() && acc.services.len() < MAX_SERVICES_PER_WATERMARK {
                acc.services.insert(trimmed.to_string());
            }
        }
    }
    by_tenant
        .into_iter()
        .map(|(tenant_id, acc)| acc.into_event(tenant_id, "metrics"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_of_spans_coalesces_to_one_watermark_per_tenant() {
        let tenant_a = Uuid::from_u128(1);
        let tenant_b = Uuid::from_u128(2);
        let mut spans = Vec::new();
        for i in 0..250u64 {
            spans.push(Span {
                tenant_id: tenant_a,
                service_name: if i % 2 == 0 {
                    "checkout".into()
                } else {
                    "payments".into()
                },
                start_time_unix_nano: 1_000 + i,
                end_time_unix_nano: 2_000 + i,
                ..Default::default()
            });
        }
        spans.push(Span {
            tenant_id: tenant_b,
            service_name: "Search".into(),
            start_time_unix_nano: 5_000,
            end_time_unix_nano: 9_500,
            ..Default::default()
        });

        let events = coalesce_span_watermarks(&spans);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            TelemetryWatermarkEvent {
                tenant_id: tenant_a,
                signal: "spans".into(),
                services: vec!["checkout".into(), "payments".into()],
                watermark_unix_nano: 2_249,
                coalesced_records: 250,
            }
        );
        assert_eq!(
            events[1],
            TelemetryWatermarkEvent {
                tenant_id: tenant_b,
                signal: "spans".into(),
                services: vec!["Search".into()],
                watermark_unix_nano: 9_500,
                coalesced_records: 1,
            }
        );
    }

    #[test]
    fn services_list_is_bounded_by_max_services_per_watermark() {
        let tenant = Uuid::from_u128(1);
        let logs: Vec<LogRecord> = (0..(MAX_SERVICES_PER_WATERMARK + 20))
            .map(|i| LogRecord {
                tenant_id: tenant,
                service_name: format!("svc-{i:03}"),
                timestamp_unix_nano: 100 + i as u64,
                ..Default::default()
            })
            .collect();

        let events = coalesce_log_watermarks(&logs);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].services.len(), MAX_SERVICES_PER_WATERMARK);
        assert_eq!(
            events[0].coalesced_records,
            (MAX_SERVICES_PER_WATERMARK + 20) as u64
        );
    }
}
