use domain::TelemetryEnvelope;
use rdkafka::{
    ClientConfig, Message, Offset, TopicPartitionList,
    consumer::{CommitMode, Consumer, StreamConsumer},
};
use std::collections::HashMap;
use std::future::Future;
use std::time::Duration;

/// Initial retry-with-backoff delay after a batch handler returns `Err`.
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(500);
/// Cap on retry-with-backoff delay; retries continue indefinitely beyond this
/// at a fixed interval rather than growing unbounded.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(30);

pub struct QueueConsumer {
    consumer: StreamConsumer,
    topic: String,
}

impl QueueConsumer {
    pub fn new(brokers: &str, group_id: &str, topic: &str) -> anyhow::Result<Self> {
        let consumer: StreamConsumer = ClientConfig::new()
            .set("bootstrap.servers", brokers)
            .set("group.id", group_id)
            .set("auto.offset.reset", "earliest")
            // Commit is driven manually, only after a batch's handler succeeds
            // (see run_batch), so a storage/downstream outage backpressures
            // instead of silently advancing past unprocessed messages.
            .set("enable.auto.commit", "false")
            .create()?;
        consumer.subscribe(&[topic])?;
        Ok(Self {
            consumer,
            topic: topic.to_string(),
        })
    }

    /// Commits the highest offset seen per partition since the last commit.
    /// Uses `CommitMode::Async`: the request is enqueued and this call
    /// returns immediately rather than blocking the async loop on a broker
    /// round-trip (`CommitMode::Sync` blocks the calling thread -- see
    /// rdkafka's `CommitMode` docs). A lost async commit is harmless here:
    /// the next successful flush commits a newer, superseding offset.
    fn commit(&self, watermark: &HashMap<i32, i64>) -> anyhow::Result<()> {
        if watermark.is_empty() {
            return Ok(());
        }
        let mut tpl = TopicPartitionList::new();
        for (&partition, &offset) in watermark {
            tpl.add_partition_offset(&self.topic, partition, Offset::Offset(offset + 1))?;
        }
        self.consumer.commit(&tpl, CommitMode::Async)?;
        Ok(())
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
        // Highest offset consumed per partition since the last commit. Updated
        // for every message pulled off the topic, including ones that fail to
        // deserialise (those can never succeed on retry, so they must not
        // block the watermark from advancing).
        let mut watermark: HashMap<i32, i64> = HashMap::new();
        let mut interval = tokio::time::interval(max_wait);
        interval.tick().await; // consume the immediate first tick
        loop {
            tokio::select! {
                result = self.consumer.recv() => {
                    let msg = result?;
                    watermark.insert(msg.partition(), msg.offset());
                    if let Some(payload) = msg.payload() {
                        match serde_json::from_slice::<TelemetryEnvelope>(payload) {
                            Ok(env) => {
                                buf.push(env);
                                if buf.len() >= max_size {
                                    let batch = std::mem::replace(
                                        &mut buf,
                                        Vec::with_capacity(max_size),
                                    );
                                    flush_with_retry(&mut handler, batch).await;
                                    self.commit(&watermark)?;
                                    watermark.clear();
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
                        flush_with_retry(&mut handler, batch).await;
                    }
                    self.commit(&watermark)?;
                    watermark.clear();
                }
            }
        }
    }
}

/// Retries `handler` with exponential backoff until it succeeds. Used by both
/// `run_batch` (real Kafka offsets committed only after this returns) and the
/// test-only `accumulate` helper below, so retry behaviour is covered by the
/// same unit tests that exercise batching.
async fn flush_with_retry<F, Fut>(handler: &mut F, batch: Vec<TelemetryEnvelope>)
where
    F: FnMut(Vec<TelemetryEnvelope>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut backoff = INITIAL_RETRY_BACKOFF;
    loop {
        match handler(batch.clone()).await {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    backoff_ms = backoff.as_millis() as u64,
                    "batch handler failed; retrying"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_RETRY_BACKOFF);
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
                            flush_with_retry(&mut handler, batch).await;
                            interval.reset();
                        }
                    }
                    None => {
                        // Channel closed — flush remaining items and return
                        if !buf.is_empty() {
                            flush_with_retry(&mut handler, std::mem::take(&mut buf)).await;
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
                    flush_with_retry(&mut handler, batch).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{EnvelopePayload, TelemetryEnvelope};
    use std::sync::atomic::{AtomicUsize, Ordering};
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

    #[tokio::test]
    async fn flush_retries_until_handler_succeeds() {
        // Real (unpaused) time: two retries at INITIAL_RETRY_BACKOFF (500ms)
        // and 2x that cost ~1.5s wall-clock, which is acceptable for a single
        // test and avoids the fragility of coordinating tokio::time::pause()
        // advances with a concurrently spawned task.
        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        let batches: Arc<Mutex<Vec<Vec<TelemetryEnvelope>>>> = Arc::new(Mutex::new(vec![]));
        let batches2 = batches.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<TelemetryEnvelope>();
        tx.send(make_env()).unwrap();
        tx.send(make_env()).unwrap();
        tx.send(make_env()).unwrap();
        drop(tx);

        accumulate(&mut rx, 3, Duration::from_secs(60), move |batch| {
            let attempts = attempts2.clone();
            let batches = batches2.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    anyhow::bail!("simulated storage outage");
                }
                batches.lock().unwrap().push(batch);
                Ok(())
            }
        })
        .await
        .unwrap();

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "handler retried until it succeeded on the 3rd attempt"
        );
        let b = batches.lock().unwrap();
        assert_eq!(b.len(), 1, "exactly one successful flush recorded");
        assert_eq!(
            b[0].len(),
            3,
            "the retried batch still contains all 3 envelopes"
        );
    }
}
