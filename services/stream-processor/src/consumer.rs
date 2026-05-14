use domain::TelemetryEnvelope;
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
};

pub struct QueueConsumer {
    consumer: StreamConsumer,
}

impl QueueConsumer {
    pub fn new(brokers: &str, group_id: &str, topic: &str) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            .create()?;
        consumer.subscribe(&[topic])?;
        Ok(Self { consumer })
    }

    pub async fn run<F, Fut>(&self, mut handler: F) -> anyhow::Result<()>
    where
        F: FnMut(TelemetryEnvelope) -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<()>>,
    {
        loop {
            let msg = self.consumer.recv().await?;
            if let Some(payload) = msg.payload() {
                match serde_json::from_slice::<TelemetryEnvelope>(payload) {
                    Ok(env) => {
                        if let Err(e) = handler(env).await {
                            tracing::warn!(error = %e, "handler error");
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "envelope deserialise failed"),
                }
            }
        }
    }
}
