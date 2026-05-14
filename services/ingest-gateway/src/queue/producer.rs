use domain::{EnvelopePayload, TelemetryEnvelope};
use rdkafka::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use std::time::Duration;
use uuid::Uuid;

pub struct QueueProducer {
    producer: FutureProducer,
    topic: String,
}

impl QueueProducer {
    pub fn new(brokers: &str, topic: &str) -> anyhow::Result<Self> {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("message.timeout.ms", "5000")
            .create()?;
        Ok(Self {
            producer,
            topic: topic.into(),
        })
    }

    pub async fn publish(&self, envelope: &TelemetryEnvelope) -> anyhow::Result<()> {
        let payload = serde_json::to_vec(envelope)?;
        let key = envelope.tenant_id.to_string();
        self.producer
            .send(
                FutureRecord::to(&self.topic).key(&key).payload(&payload),
                Duration::from_secs(5),
            )
            .await
            .map_err(|(e, _)| anyhow::anyhow!("kafka send error: {e}"))?;
        Ok(())
    }
}

pub fn build_envelope(
    tenant_id: Uuid,
    environment: &str,
    payload: EnvelopePayload,
) -> TelemetryEnvelope {
    TelemetryEnvelope {
        envelope_id: Uuid::new_v4(),
        tenant_id,
        environment: environment.to_string(),
        received_at_unix_nano: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64,
        payload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_serializes_for_kafka() {
        let env = build_envelope(
            Uuid::new_v4(),
            "test",
            domain::EnvelopePayload::Spans(vec![]),
        );
        let bytes = serde_json::to_vec(&env).unwrap();
        assert!(!bytes.is_empty());
    }
}
