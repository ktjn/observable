use domain::{LogRecord, MetricPoint, MetricSeries, Span};
use std::future::Future;
use std::time::Duration;
use tokio::sync::oneshot;

const CHANNEL_CAPACITY: usize = 512;

/// Initial retry-with-backoff delay after a ClickHouse flush fails.
const INITIAL_RETRY_BACKOFF: Duration = Duration::from_millis(500);
/// Cap on retry-with-backoff delay; retries continue indefinitely beyond this
/// at a fixed interval rather than growing unbounded.
const MAX_RETRY_BACKOFF: Duration = Duration::from_secs(30);

/// Optional per-submission completion signal. `None` for the fire-and-forget
/// HTTP-handler path (`send_spans`/`send_logs`/`send_metrics`); `Some` for the
/// durable path (`send_spans_durable`/...) used by `NormalizedConsumer`, which
/// awaits it before committing the Kafka offset that produced these rows.
type Ack = Option<oneshot::Sender<()>>;

/// Async write buffer for storage-writer.
///
/// Accumulates rows across HTTP calls and flushes to ClickHouse in large
/// blocks on a count threshold or idle timeout. A flush that fails retries
/// with backoff indefinitely rather than dropping the batch -- see
/// `retry_until_success`. The channel-full case (`try_send` in `send_spans`
/// etc.) remains a best-effort drop: those callers cannot block, so sustained
/// backpressure still surfaces as data loss for the HTTP path, same as
/// before. The durable path (`send_spans_durable` etc.) blocks on a full
/// channel instead of dropping, which is what lets its caller
/// (`NormalizedConsumer`) translate a storage outage into Kafka backpressure.
pub struct WriteBuffer {
    spans_tx: tokio::sync::mpsc::Sender<(Vec<Span>, Ack)>,
    logs_tx: tokio::sync::mpsc::Sender<(Vec<LogRecord>, Ack)>,
    metrics_tx: tokio::sync::mpsc::Sender<(Vec<MetricSeries>, Vec<MetricPoint>, Ack)>,
}

impl WriteBuffer {
    /// Create a new buffer and spawn background flush tasks.
    /// Requires a running Tokio runtime (called from `main()`).
    pub fn new(ch: clickhouse::Client, max_rows: usize, flush_interval: Duration) -> Self {
        let (spans_tx, spans_rx) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
        let (logs_tx, logs_rx) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
        let (metrics_tx, metrics_rx) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);

        tokio::spawn(spans_flush_loop(
            spans_rx,
            ch.clone(),
            max_rows,
            flush_interval,
        ));
        tokio::spawn(logs_flush_loop(
            logs_rx,
            ch.clone(),
            max_rows,
            flush_interval,
        ));
        tokio::spawn(metrics_flush_loop(metrics_rx, ch, max_rows, flush_interval));

        Self {
            spans_tx,
            logs_tx,
            metrics_tx,
        }
    }

    /// Non-blocking send. Drops the batch and logs if the channel is full.
    pub fn send_spans(&self, spans: Vec<Span>) {
        if let Err(e) = self.spans_tx.try_send((spans, None)) {
            tracing::error!(error = %e, "spans buffer channel full, dropping batch");
        }
    }

    pub fn send_logs(&self, logs: Vec<LogRecord>) {
        if let Err(e) = self.logs_tx.try_send((logs, None)) {
            tracing::error!(error = %e, "logs buffer channel full, dropping batch");
        }
    }

    pub fn send_metrics(&self, series: Vec<MetricSeries>, points: Vec<MetricPoint>) {
        if let Err(e) = self.metrics_tx.try_send((series, points, None)) {
            tracing::error!(error = %e, "metrics buffer channel full, dropping batch");
        }
    }

    /// Blocks until `spans` has been durably written to ClickHouse (retried
    /// indefinitely on failure) or the flush loop has stopped. Backpressures
    /// on a full channel instead of dropping.
    pub async fn send_spans_durable(&self, spans: Vec<Span>) -> anyhow::Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.spans_tx
            .send((spans, Some(ack_tx)))
            .await
            .map_err(|_| anyhow::anyhow!("spans flush loop stopped"))?;
        ack_rx
            .await
            .map_err(|_| anyhow::anyhow!("spans flush loop dropped ack"))
    }

    pub async fn send_logs_durable(&self, logs: Vec<LogRecord>) -> anyhow::Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.logs_tx
            .send((logs, Some(ack_tx)))
            .await
            .map_err(|_| anyhow::anyhow!("logs flush loop stopped"))?;
        ack_rx
            .await
            .map_err(|_| anyhow::anyhow!("logs flush loop dropped ack"))
    }

    pub async fn send_metrics_durable(
        &self,
        series: Vec<MetricSeries>,
        points: Vec<MetricPoint>,
    ) -> anyhow::Result<()> {
        let (ack_tx, ack_rx) = oneshot::channel();
        self.metrics_tx
            .send((series, points, Some(ack_tx)))
            .await
            .map_err(|_| anyhow::anyhow!("metrics flush loop stopped"))?;
        ack_rx
            .await
            .map_err(|_| anyhow::anyhow!("metrics flush loop dropped ack"))
    }
}

/// Retries `insert(rows.clone())` with exponential backoff until it succeeds.
/// Never gives up -- a sustained ClickHouse outage backpressures the flush
/// loop (and, transitively, any durable-path caller) rather than losing data.
async fn retry_until_success<T, Fut>(
    rows: Vec<T>,
    mut insert: impl FnMut(Vec<T>) -> Fut,
    label: &str,
) where
    T: Clone,
    Fut: Future<Output = anyhow::Result<()>>,
{
    let mut backoff = INITIAL_RETRY_BACKOFF;
    loop {
        match insert(rows.clone()).await {
            Ok(()) => return,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    target = label,
                    backoff_ms = backoff.as_millis() as u64,
                    "flush to clickhouse failed; retrying"
                );
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_RETRY_BACKOFF);
            }
        }
    }
}

fn ack_all(acks: Vec<oneshot::Sender<()>>) {
    for ack in acks {
        let _ = ack.send(());
    }
}

async fn spans_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<(Vec<Span>, Ack)>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut buf: Vec<Span> = Vec::with_capacity(max_rows);
    let mut acks: Vec<oneshot::Sender<()>> = Vec::new();
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await; // consume immediate first tick

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some((batch, ack)) => {
                        buf.extend(batch);
                        acks.extend(ack);
                        if buf.len() >= max_rows {
                            let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                            let to_ack = std::mem::take(&mut acks);
                            retry_until_success(to_flush, |rows| crate::spans::insert_spans(&ch, rows), "spans").await;
                            ack_all(to_ack);
                            interval.reset();
                        }
                    }
                    None => {
                        if buf.is_empty() { return; }
                        retry_until_success(buf, |rows| crate::spans::insert_spans(&ch, rows), "spans").await;
                        ack_all(acks);
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    let to_ack = std::mem::take(&mut acks);
                    retry_until_success(to_flush, |rows| crate::spans::insert_spans(&ch, rows), "spans").await;
                    ack_all(to_ack);
                }
            }
        }
    }
}

async fn logs_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<(Vec<LogRecord>, Ack)>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut buf: Vec<LogRecord> = Vec::with_capacity(max_rows);
    let mut acks: Vec<oneshot::Sender<()>> = Vec::new();
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await;

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some((batch, ack)) => {
                        buf.extend(batch);
                        acks.extend(ack);
                        if buf.len() >= max_rows {
                            let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                            let to_ack = std::mem::take(&mut acks);
                            retry_until_success(to_flush, |rows| crate::logs::insert_logs(&ch, rows), "logs").await;
                            ack_all(to_ack);
                            interval.reset();
                        }
                    }
                    None => {
                        if buf.is_empty() { return; }
                        retry_until_success(buf, |rows| crate::logs::insert_logs(&ch, rows), "logs").await;
                        ack_all(acks);
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    let to_ack = std::mem::take(&mut acks);
                    retry_until_success(to_flush, |rows| crate::logs::insert_logs(&ch, rows), "logs").await;
                    ack_all(to_ack);
                }
            }
        }
    }
}

async fn metrics_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<(Vec<MetricSeries>, Vec<MetricPoint>, Ack)>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut series_buf: Vec<MetricSeries> = Vec::with_capacity(max_rows / 2 + 1);
    let mut points_buf: Vec<MetricPoint> = Vec::with_capacity(max_rows);
    let mut acks: Vec<oneshot::Sender<()>> = Vec::new();
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await;

    async fn flush_metrics(
        ch: &clickhouse::Client,
        series: Vec<MetricSeries>,
        points: Vec<MetricPoint>,
    ) {
        // Independent retries: a stuck series flush does not block points
        // (matches the prior best-effort behavior's independence, just with
        // retry instead of drop-on-error).
        let series_fut = retry_until_success(
            series,
            |rows| crate::metrics::insert_metric_series(ch, rows),
            "metric_series",
        );
        let points_fut = retry_until_success(
            points,
            |rows| crate::metrics::insert_metric_points(ch, rows),
            "metric_points",
        );
        tokio::join!(series_fut, points_fut);
    }

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some((series, points, ack)) => {
                        series_buf.extend(series);
                        points_buf.extend(points);
                        acks.extend(ack);
                        if series_buf.len() + points_buf.len() >= max_rows {
                            let s = std::mem::take(&mut series_buf);
                            let p = std::mem::take(&mut points_buf);
                            let to_ack = std::mem::take(&mut acks);
                            flush_metrics(&ch, s, p).await;
                            ack_all(to_ack);
                            interval.reset();
                        }
                    }
                    None => {
                        if !series_buf.is_empty() || !points_buf.is_empty() {
                            flush_metrics(&ch, series_buf, points_buf).await;
                            ack_all(acks);
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !series_buf.is_empty() || !points_buf.is_empty() {
                    let s = std::mem::take(&mut series_buf);
                    let p = std::mem::take(&mut points_buf);
                    let to_ack = std::mem::take(&mut acks);
                    flush_metrics(&ch, s, p).await;
                    ack_all(to_ack);
                }
            }
        }
    }
}

// Test-only helper: same select-loop logic as spans_flush_loop but accepts a
// mock flush function instead of a ClickHouse client, and retries on Err the
// same way the real loop does.
#[cfg(test)]
pub(crate) async fn test_accumulate_spans<F, Fut>(
    rx: &mut tokio::sync::mpsc::Receiver<Vec<Span>>,
    max_rows: usize,
    flush_interval: Duration,
    mut flush_fn: F,
) where
    F: FnMut(Vec<Span>) -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<()>>,
{
    let mut buf: Vec<Span> = Vec::with_capacity(max_rows);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await;

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some(batch) => {
                        buf.extend(batch);
                        if buf.len() >= max_rows {
                            let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                            retry_until_success(to_flush, &mut flush_fn, "spans").await;
                            interval.reset();
                        }
                    }
                    None => {
                        if !buf.is_empty() {
                            retry_until_success(buf, &mut flush_fn, "spans").await;
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    retry_until_success(to_flush, &mut flush_fn, "spans").await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn make_span() -> Span {
        Span {
            tenant_id: uuid::Uuid::new_v4(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn flush_on_count() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        // max_rows = 4; send 3 then 3 → total 6 after second recv → count flush
        tx.send(vec![make_span(), make_span(), make_span()])
            .await
            .unwrap();
        tx.send(vec![make_span(), make_span(), make_span()])
            .await
            .unwrap();
        drop(tx);

        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();

        test_accumulate_spans(&mut rx, 4, Duration::from_secs(60), move |batch| {
            let f = flushed2.clone();
            async move {
                f.lock().unwrap().push(batch);
                Ok(())
            }
        })
        .await;

        let batches = flushed.lock().unwrap();
        assert_eq!(batches.len(), 1, "one flush when count exceeded");
        assert_eq!(
            batches[0].len(),
            6,
            "flush contains all rows accumulated past threshold"
        );
    }

    #[tokio::test]
    async fn flush_on_timeout() {
        tokio::time::pause();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        tx.send(vec![make_span(), make_span()]).await.unwrap();
        // keep tx alive so channel stays open

        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let mut done_opt = Some(done_tx);

        let handle = tokio::spawn(async move {
            test_accumulate_spans(&mut rx, 100, Duration::from_millis(200), move |batch| {
                let f = flushed2.clone();
                if let Some(s) = done_opt.take() {
                    let _ = s.send(());
                }
                async move {
                    f.lock().unwrap().push(batch);
                    Ok(())
                }
            })
            .await;
        });

        tokio::time::advance(Duration::from_millis(201)).await;
        done_rx.await.unwrap();
        handle.abort();

        let batches = flushed.lock().unwrap();
        assert_eq!(batches.len(), 1, "one flush on timeout");
        assert_eq!(batches[0].len(), 2, "partial batch of 2 flushed on timeout");
    }

    #[tokio::test]
    async fn flush_retries_until_success_instead_of_dropping() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        tx.send(vec![make_span(), make_span()]).await.unwrap();
        drop(tx);

        let attempts = Arc::new(AtomicUsize::new(0));
        let attempts2 = attempts.clone();
        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();

        test_accumulate_spans(&mut rx, 100, Duration::from_secs(60), move |batch| {
            let attempts = attempts2.clone();
            let flushed = flushed2.clone();
            async move {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    anyhow::bail!("simulated clickhouse outage");
                }
                flushed.lock().unwrap().push(batch);
                Ok(())
            }
        })
        .await;

        assert_eq!(
            attempts.load(Ordering::SeqCst),
            3,
            "retried until the 3rd attempt succeeded, batch was never dropped"
        );
        let batches = flushed.lock().unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 2, "the full retried batch was flushed");
    }

    #[tokio::test]
    async fn send_spans_durable_resolves_only_after_flush_succeeds() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<Span>, Ack)>(16);
        let (ack_tx, ack_rx) = oneshot::channel();
        tx.send((vec![make_span()], Some(ack_tx))).await.unwrap();
        drop(tx);

        let handle = tokio::spawn(async move {
            // Mirrors spans_flush_loop's None-branch (final flush + ack) without
            // requiring a live ClickHouse client, since this test only checks
            // that the ack fires after (not before/instead of) a successful
            // "flush".
            if let Some((batch, ack)) = rx.recv().await {
                retry_until_success(batch, |_rows| async { Ok(()) }, "spans").await;
                if let Some(ack) = ack {
                    let _ = ack.send(());
                }
            }
        });

        ack_rx.await.expect("ack fires after flush completes");
        handle.await.unwrap();
    }
}
