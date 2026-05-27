use domain::TelemetryEnvelope;
use rdkafka::{
    ClientConfig, Message,
    consumer::{Consumer, StreamConsumer},
};
use std::future::Future;
use std::time::Duration;

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

    pub async fn run_batch<F, Fut>(
        &self,
        max_size: usize,
        max_wait: Duration,
        mut handler: F,
    ) -> anyhow::Result<()>
    where
        F: FnMut(Vec<TelemetryEnvelope>) -> Fut,
        Fut: Future<Output = anyhow::Result<()>>,
    {
        let mut buf: Vec<TelemetryEnvelope> = Vec::with_capacity(max_size);
        let mut interval = tokio::time::interval(max_wait);
        interval.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                result = self.consumer.recv() => {
                    let msg = result?;
                    if let Some(payload) = msg.payload() {
                        match serde_json::from_slice::<TelemetryEnvelope>(payload) {
                            Ok(env) => {
                                buf.push(env);
                                if buf.len() >= max_size {
                                    let batch = std::mem::replace(
                                        &mut buf,
                                        Vec::with_capacity(max_size),
                                    );
                                    if let Err(e) = handler(batch).await {
                                        tracing::warn!(error = %e, "batch handler error");
                                    }
                                    interval.reset();
                                }
                            }
                            Err(e) => tracing::warn!(error = %e, "envelope deserialise failed"),
                        }
                    }
                }
                _ = interval.tick() => {
                    if !buf.is_empty() {
                        let batch = std::mem::replace(
                            &mut buf,
                            Vec::with_capacity(max_size),
                        );
                        if let Err(e) = handler(batch).await {
                            tracing::warn!(error = %e, "batch handler error");
                        }
                    }
                }
            }
        }
    }
}

// Test-only helper: same select-loop logic as run_batch but reads from an mpsc channel
// so tests exercise count/timeout batching without a real Kafka connection.
#[cfg(test)]
async fn accumulate<F, Fut>(
    rx: &mut tokio::sync::mpsc::UnboundedReceiver<TelemetryEnvelope>,
    max_size: usize,
    max_wait: Duration,
    mut handler: F,
) -> anyhow::Result<()>
where
    F: FnMut(Vec<TelemetryEnvelope>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut buf: Vec<TelemetryEnvelope> = Vec::with_capacity(max_size);
    let mut interval = tokio::time::interval(max_wait);
    interval.tick().await;
    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some(env) => {
                        buf.push(env);
                        if buf.len() >= max_size {
                            let batch = std::mem::replace(
                                &mut buf,
                                Vec::with_capacity(max_size),
                            );
                            if let Err(e) = handler(batch).await {
                                tracing::warn!(error = %e, "batch handler error");
                            }
                            interval.reset();
                        }
                    }
                    None => {
                        // Channel closed — flush remaining items and return
                        if !buf.is_empty()
                            && let Err(e) = handler(std::mem::take(&mut buf)).await
                        {
                            tracing::warn!(error = %e, "batch handler error");
                        }
                        return Ok(());
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let batch = std::mem::replace(
                        &mut buf,
                        Vec::with_capacity(max_size),
                    );
                    if let Err(e) = handler(batch).await {
                        tracing::warn!(error = %e, "batch handler error");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EnvelopePayload, TelemetryEnvelope};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use uuid::Uuid;

    fn make_env() -> TelemetryEnvelope {
        TelemetryEnvelope {
            envelope_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            environment: "prod".into(),
            received_at_unix_nano: 0,
            payload: EnvelopePayload::Spans(vec![]),
        }
    }

    #[tokio::test]
    async fn flush_on_count() {
        let batches: Arc<Mutex<Vec<Vec<TelemetryEnvelope>>>> = Arc::new(Mutex::new(vec![]));
        let batches2 = batches.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TelemetryEnvelope>();
        for _ in 0..3 {
            tx.send(make_env()).unwrap();
        }
        drop(tx); // close channel so accumulate returns after the flush

        accumulate(&mut rx, 3, Duration::from_secs(60), move |batch| {
            let b = batches2.clone();
            async move {
                b.lock().unwrap().push(batch);
                Ok(())
            }
        })
        .await
        .unwrap();

        let b = batches.lock().unwrap();
        assert_eq!(b.len(), 1, "handler called exactly once");
        assert_eq!(b[0].len(), 3, "batch contains all 3 envelopes");
    }

    #[tokio::test]
    async fn flush_on_timeout() {
        tokio::time::pause();

        let batches: Arc<Mutex<Vec<Vec<TelemetryEnvelope>>>> = Arc::new(Mutex::new(vec![]));
        let batches2 = batches.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TelemetryEnvelope>();
        for _ in 0..2 {
            tx.send(make_env()).unwrap();
        }
        // Keep tx alive so channel stays open; accumulate blocks after 2 messages

        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let mut done_opt = Some(done_tx);

        let handle = tokio::spawn(async move {
            accumulate(&mut rx, 10, Duration::from_millis(200), move |batch| {
                let b = batches2.clone();
                if let Some(s) = done_opt.take() {
                    let _ = s.send(());
                }
                async move {
                    b.lock().unwrap().push(batch);
                    Ok(())
                }
            })
            .await
        });

        tokio::time::advance(Duration::from_millis(201)).await;
        done_rx.await.unwrap();
        handle.abort();

        let b = batches.lock().unwrap();
        assert_eq!(b.len(), 1, "handler called once by timer");
        assert_eq!(b[0].len(), 2, "partial batch of 2 flushed");
    }
}
