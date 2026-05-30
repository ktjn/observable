use domain::{LogRecord, MetricPoint, MetricSeries, Span};
use std::time::Duration;

const CHANNEL_CAPACITY: usize = 512;

/// Async write buffer for storage-writer.
///
/// Accumulates rows across HTTP calls and flushes to ClickHouse in large
/// blocks on a count threshold or idle timeout. Flush errors are logged
/// and the batch is dropped — observability data is best-effort.
pub struct WriteBuffer {
    spans_tx: tokio::sync::mpsc::Sender<Vec<Span>>,
    logs_tx: tokio::sync::mpsc::Sender<Vec<LogRecord>>,
    metrics_tx: tokio::sync::mpsc::Sender<(Vec<MetricSeries>, Vec<MetricPoint>)>,
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
        if let Err(e) = self.spans_tx.try_send(spans) {
            tracing::error!(error = %e, "spans buffer channel full, dropping batch");
        }
    }

    pub fn send_logs(&self, logs: Vec<LogRecord>) {
        if let Err(e) = self.logs_tx.try_send(logs) {
            tracing::error!(error = %e, "logs buffer channel full, dropping batch");
        }
    }

    pub fn send_metrics(&self, series: Vec<MetricSeries>, points: Vec<MetricPoint>) {
        if let Err(e) = self.metrics_tx.try_send((series, points)) {
            tracing::error!(error = %e, "metrics buffer channel full, dropping batch");
        }
    }
}

async fn spans_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<Vec<Span>>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut buf: Vec<Span> = Vec::with_capacity(max_rows);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await; // consume immediate first tick

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some(batch) => {
                        buf.extend(batch);
                        if buf.len() >= max_rows {
                            let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                            if let Err(e) = crate::spans::insert_spans(&ch, to_flush).await {
                                tracing::error!(error = %e, "flush spans to clickhouse failed");
                            }
                            interval.reset();
                        }
                    }
                    None => {
                        if buf.is_empty() { return; }
                        if let Err(e) = crate::spans::insert_spans(&ch, buf).await {
                            tracing::error!(error = %e, "final flush spans to clickhouse failed");
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    if let Err(e) = crate::spans::insert_spans(&ch, to_flush).await {
                        tracing::error!(error = %e, "flush spans to clickhouse failed");
                    }
                }
            }
        }
    }
}

async fn logs_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<Vec<LogRecord>>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut buf: Vec<LogRecord> = Vec::with_capacity(max_rows);
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
                            if let Err(e) = crate::logs::insert_logs(&ch, to_flush).await {
                                tracing::error!(error = %e, "flush logs to clickhouse failed");
                            }
                            interval.reset();
                        }
                    }
                    None => {
                        if buf.is_empty() { return; }
                        if let Err(e) = crate::logs::insert_logs(&ch, buf).await {
                            tracing::error!(error = %e, "final flush logs to clickhouse failed");
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    if let Err(e) = crate::logs::insert_logs(&ch, to_flush).await {
                        tracing::error!(error = %e, "flush logs to clickhouse failed");
                    }
                }
            }
        }
    }
}

async fn metrics_flush_loop(
    mut rx: tokio::sync::mpsc::Receiver<(Vec<MetricSeries>, Vec<MetricPoint>)>,
    ch: clickhouse::Client,
    max_rows: usize,
    flush_interval: Duration,
) {
    let mut series_buf: Vec<MetricSeries> = Vec::with_capacity(max_rows / 2 + 1);
    let mut points_buf: Vec<MetricPoint> = Vec::with_capacity(max_rows);
    let mut interval = tokio::time::interval(flush_interval);
    interval.tick().await;

    loop {
        tokio::select! {
            item = rx.recv() => {
                match item {
                    Some((series, points)) => {
                        series_buf.extend(series);
                        points_buf.extend(points);
                        if series_buf.len() + points_buf.len() >= max_rows {
                            let s = std::mem::take(&mut series_buf);
                            let p = std::mem::take(&mut points_buf);
                            // Best-effort: flush series and points independently.
                            // A series failure does not suppress the points flush.
                            if let Err(e) = crate::metrics::insert_metric_series(&ch, s).await {
                                tracing::error!(error = %e, "flush metric_series to clickhouse failed");
                            }
                            if let Err(e) = crate::metrics::insert_metric_points(&ch, p).await {
                                tracing::error!(error = %e, "flush metric_points to clickhouse failed");
                            }
                            interval.reset();
                        }
                    }
                    None => {
                        if !series_buf.is_empty() || !points_buf.is_empty() {
                            // Best-effort: flush series and points independently.
                            // A series failure does not suppress the points flush.
                            if let Err(e) = crate::metrics::insert_metric_series(&ch, series_buf).await {
                                tracing::error!(error = %e, "final flush metric_series failed");
                            }
                            if let Err(e) = crate::metrics::insert_metric_points(&ch, points_buf).await {
                                tracing::error!(error = %e, "final flush metric_points failed");
                            }
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !series_buf.is_empty() || !points_buf.is_empty() {
                    let s = std::mem::take(&mut series_buf);
                    let p = std::mem::take(&mut points_buf);
                    // Best-effort: flush series and points independently.
                    // A series failure does not suppress the points flush.
                    if let Err(e) = crate::metrics::insert_metric_series(&ch, s).await {
                        tracing::error!(error = %e, "flush metric_series to clickhouse failed");
                    }
                    if let Err(e) = crate::metrics::insert_metric_points(&ch, p).await {
                        tracing::error!(error = %e, "flush metric_points to clickhouse failed");
                    }
                }
            }
        }
    }
}

// Test-only helper: same select-loop logic as spans_flush_loop but accepts a
// mock flush function instead of a ClickHouse client.
// Follows the stream-processor `accumulate` pattern exactly.
#[cfg(test)]
pub(crate) async fn test_accumulate_spans<F, Fut>(
    rx: &mut tokio::sync::mpsc::Receiver<Vec<Span>>,
    max_rows: usize,
    flush_interval: Duration,
    mut flush_fn: F,
) where
    F: FnMut(Vec<Span>) -> Fut,
    Fut: std::future::Future<Output = ()>,
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
                            flush_fn(to_flush).await;
                            interval.reset();
                        }
                    }
                    None => {
                        if !buf.is_empty() {
                            flush_fn(buf).await;
                        }
                        return;
                    }
                }
            }
            _ = interval.tick() => {
                if !buf.is_empty() {
                    let to_flush = std::mem::replace(&mut buf, Vec::with_capacity(max_rows));
                    flush_fn(to_flush).await;
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
}
