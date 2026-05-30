# ClickHouse Insert Efficiency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an async `WriteBuffer` to `storage-writer` that accumulates rows from multiple HTTP calls and flushes to ClickHouse in large blocks rather than one INSERT per request.

**Architecture:** A new `buffer.rs` lib module holds three `tokio::mpsc` channels (spans, logs, metrics) and three background flush tasks. HTTP handlers push to the channels and return 204 immediately; flush tasks fire on row-count threshold (`STORAGE_WRITER_FLUSH_MAX_ROWS`, default 5 000) or idle timeout (`STORAGE_WRITER_FLUSH_INTERVAL_MS`, default 500 ms). The existing `insert_spans`/`insert_logs`/`insert_metric_*` functions remain unchanged — the buffer calls them on flush.

**Tech Stack:** Rust, Tokio (`mpsc`, `time::interval`, `select!`), `clickhouse` crate.

**Design doc:** `docs/superpowers/specs/2026-05-30-clickhouse-insert-efficiency-design.md`

---

## Files Changed

| File | Change |
|---|---|
| `services/storage-writer/src/lib.rs` | Add `pub mod buffer; pub mod spans; pub mod logs; pub mod metrics;`; update `AppState` |
| `services/storage-writer/src/buffer.rs` | New: `WriteBuffer` struct + three flush loops + `#[cfg(test)]` accumulate helper + unit tests |
| `services/storage-writer/src/main.rs` | Remove `mod spans; mod logs; mod metrics;`; use buffer in handlers; add env-var config |
| `tests/e2e/smoke_test_unit.sh` | Assert `buffer.rs` exists |
| `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md` | Mark ClickHouse Insert Efficiency complete |
| `docs/agent-context.md` | Update active plan note |

---

## Task 1: Move insert modules to `lib.rs` and update `AppState`

Currently `spans.rs`, `logs.rs`, `metrics.rs` are declared `mod` inside `main.rs` (binary-only). `buffer.rs` will live in the lib crate and must call `insert_spans` etc., so the insert modules must also be lib-crate modules. This task moves the declarations — no code inside those files changes.

**Files:**
- Modify: `services/storage-writer/src/lib.rs`
- Modify: `services/storage-writer/src/main.rs`

- [ ] **Step 1: Update `lib.rs` to declare the insert modules and update `AppState`**

Replace the entire contents of `services/storage-writer/src/lib.rs`:

```rust
pub mod buffer;
pub mod logs;
pub mod metrics;
pub mod observability;
pub mod spans;

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    /// Async write buffer — handlers push here and return immediately.
    pub buffer: Arc<buffer::WriteBuffer>,
    /// Direct ClickHouse client — used by the readyz probe and retention worker only.
    pub ch: clickhouse::Client,
    pub metrics: Arc<observability::StorageWriterMetrics>,
}
```

- [ ] **Step 2: Remove the insert-module declarations from `main.rs`**

In `services/storage-writer/src/main.rs`, remove these three lines near the top:

```rust
mod logs;
mod metrics;
mod spans;
```

(`mod retention;` stays — the retention worker is binary-only.)

- [ ] **Step 3: Verify compilation**

```bash
cargo build -p storage-writer
```

Expected: compiles. The `buffer` module does not exist yet so there will be a compile error about it — that is expected and will be fixed in Task 2.

Actually, `pub mod buffer;` in `lib.rs` requires `buffer.rs` to exist. Create a placeholder first:

```bash
echo "// placeholder" > services/storage-writer/src/buffer.rs
cargo build -p storage-writer
```

Expected: clean build (the placeholder satisfies the module declaration).

- [ ] **Step 4: Run existing lib tests to confirm nothing broke**

```bash
cargo test -p storage-writer --lib
```

Expected: all existing tests pass.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add services/storage-writer/src/lib.rs \
        services/storage-writer/src/main.rs \
        services/storage-writer/src/buffer.rs
git commit -m "refactor(storage-writer): move insert modules to lib crate for shared access"
```

---

## Task 2: Create `buffer.rs` with `WriteBuffer`, flush loops, and unit tests (TDD)

**Files:**
- Create/replace: `services/storage-writer/src/buffer.rs`

- [ ] **Step 1: Write the failing unit tests**

Replace the placeholder `buffer.rs` with the tests and stub types:

```rust
use domain::{LogRecord, MetricPoint, MetricSeries, Span};
use std::time::Duration;

const CHANNEL_CAPACITY: usize = 512;

pub struct WriteBuffer {
    spans_tx: tokio::sync::mpsc::Sender<Vec<Span>>,
    logs_tx: tokio::sync::mpsc::Sender<Vec<LogRecord>>,
    metrics_tx: tokio::sync::mpsc::Sender<(Vec<MetricSeries>, Vec<MetricPoint>)>,
}

impl WriteBuffer {
    pub fn new(
        _ch: clickhouse::Client,
        _max_rows: usize,
        _flush_interval: Duration,
    ) -> Self {
        let (spans_tx, _) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
        let (logs_tx, _) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
        let (metrics_tx, _) = tokio::sync::mpsc::channel(CHANNEL_CAPACITY);
        Self { spans_tx, logs_tx, metrics_tx }
    }

    pub fn send_spans(&self, _spans: Vec<Span>) {}
    pub fn send_logs(&self, _logs: Vec<LogRecord>) {}
    pub fn send_metrics(&self, _series: Vec<MetricSeries>, _points: Vec<MetricPoint>) {}
}

// Test-only: same select-loop logic as the production flush loops but driven by an
// mpsc channel with a mock flush function. Follows the stream-processor `accumulate`
// pattern so tests exercise count/timeout batching without a real ClickHouse.
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
    // stub — will be implemented in Step 4
    let _ = (rx, max_rows, flush_interval);
    flush_fn(vec![]).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn make_span() -> Span {
        Span { tenant_id: uuid::Uuid::new_v4(), ..Default::default() }
    }

    #[tokio::test]
    async fn flush_on_count() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        // max_rows = 4; send 3 spans, then 3 more → total 6 >= 4 → one count-triggered flush
        tx.send(vec![make_span(), make_span(), make_span()]).await.unwrap();
        tx.send(vec![make_span(), make_span(), make_span()]).await.unwrap();
        drop(tx);

        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();

        test_accumulate_spans(&mut rx, 4, Duration::from_secs(60), move |batch| {
            let f = flushed2.clone();
            async move { f.lock().unwrap().push(batch); }
        })
        .await;

        let batches = flushed.lock().unwrap();
        assert_eq!(batches.len(), 1, "one flush when count exceeded");
        assert_eq!(batches[0].len(), 6, "flush contains all rows accumulated past threshold");
    }

    #[tokio::test]
    async fn flush_on_timeout() {
        tokio::time::pause();

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        tx.send(vec![make_span(), make_span()]).await.unwrap();
        // keep tx alive so channel stays open after 2 spans

        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();
        let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
        let mut done_opt = Some(done_tx);

        let handle = tokio::spawn(async move {
            test_accumulate_spans(&mut rx, 100, Duration::from_millis(200), move |batch| {
                let f = flushed2.clone();
                if let Some(s) = done_opt.take() { let _ = s.send(()); }
                async move { f.lock().unwrap().push(batch); }
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
```

- [ ] **Step 2: Run tests — verify RED**

```bash
cargo test -p storage-writer --lib -- buffer 2>&1 | tail -10
```

Expected: `flush_on_count` and `flush_on_timeout` fail (the stub `test_accumulate_spans` always calls `flush_fn(vec![])` rather than implementing the real loop).

- [ ] **Step 3: Implement `test_accumulate_spans` and the three production flush loops**

Replace `buffer.rs` with the full implementation:

```rust
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

        tokio::spawn(spans_flush_loop(spans_rx, ch.clone(), max_rows, flush_interval));
        tokio::spawn(logs_flush_loop(logs_rx, ch.clone(), max_rows, flush_interval));
        tokio::spawn(metrics_flush_loop(metrics_rx, ch, max_rows, flush_interval));

        Self { spans_tx, logs_tx, metrics_tx }
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
                        if !buf.is_empty() {
                            if let Err(e) = crate::spans::insert_spans(&ch, buf).await {
                                tracing::error!(error = %e, "final flush spans to clickhouse failed");
                            }
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
                        if !buf.is_empty() {
                            if let Err(e) = crate::logs::insert_logs(&ch, buf).await {
                                tracing::error!(error = %e, "final flush logs to clickhouse failed");
                            }
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
    let mut series_buf: Vec<MetricSeries> = Vec::new();
    let mut points_buf: Vec<MetricPoint> = Vec::new();
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
        Span { tenant_id: uuid::Uuid::new_v4(), ..Default::default() }
    }

    #[tokio::test]
    async fn flush_on_count() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<Span>>(16);
        // max_rows = 4; send 3 then 3 → total 6 after second recv → count flush
        tx.send(vec![make_span(), make_span(), make_span()]).await.unwrap();
        tx.send(vec![make_span(), make_span(), make_span()]).await.unwrap();
        drop(tx);

        let flushed: Arc<Mutex<Vec<Vec<Span>>>> = Arc::new(Mutex::new(Vec::new()));
        let flushed2 = flushed.clone();

        test_accumulate_spans(&mut rx, 4, Duration::from_secs(60), move |batch| {
            let f = flushed2.clone();
            async move { f.lock().unwrap().push(batch); }
        })
        .await;

        let batches = flushed.lock().unwrap();
        assert_eq!(batches.len(), 1, "one flush when count exceeded");
        assert_eq!(batches[0].len(), 6, "flush contains all rows accumulated past threshold");
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
                if let Some(s) = done_opt.take() { let _ = s.send(()); }
                async move { f.lock().unwrap().push(batch); }
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
```

- [ ] **Step 4: Run tests — verify GREEN**

```bash
cargo test -p storage-writer --lib -- buffer
```

Expected: `flush_on_count` and `flush_on_timeout` both pass.

- [ ] **Step 5: Run all lib tests to confirm no regressions**

```bash
cargo test -p storage-writer --lib
```

Expected: all tests pass.

- [ ] **Step 6: Format and commit**

```bash
cargo fmt --all
git add services/storage-writer/src/buffer.rs
git commit -m "feat(storage-writer): add async WriteBuffer with count/timeout flush for ClickHouse efficiency"
```

---

## Task 3: Wire `WriteBuffer` into handlers and `main.rs`

**Files:**
- Modify: `services/storage-writer/src/main.rs`

- [ ] **Step 1: Replace `write_spans`, `write_logs`, `write_metrics` handlers**

In `services/storage-writer/src/main.rs`, replace the three handler functions:

Old `write_spans`:
```rust
async fn write_spans(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::Span>>,
) -> StatusCode {
    match spans::insert_spans(&state.ch, batch).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(error = %e, "clickhouse write failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
```

New `write_spans`:
```rust
async fn write_spans(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::Span>>,
) -> StatusCode {
    state.buffer.send_spans(batch);
    StatusCode::NO_CONTENT
}
```

Old `write_logs`:
```rust
async fn write_logs(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::LogRecord>>,
) -> StatusCode {
    match logs::insert_logs(&state.ch, batch).await {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(error = %e, "ch write failed");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}
```

New `write_logs`:
```rust
async fn write_logs(
    State(state): State<AppState>,
    Json(batch): Json<Vec<domain::LogRecord>>,
) -> StatusCode {
    state.buffer.send_logs(batch);
    StatusCode::NO_CONTENT
}
```

Old `write_metrics`:
```rust
async fn write_metrics(State(state): State<AppState>, Json(b): Json<MetricsBatch>) -> StatusCode {
    let r1 = metrics::insert_metric_series(&state.ch, b.series).await;
    let r2 = metrics::insert_metric_points(&state.ch, b.points).await;
    if r1.is_err() || r2.is_err() {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::NO_CONTENT
    }
}
```

New `write_metrics`:
```rust
async fn write_metrics(State(state): State<AppState>, Json(b): Json<MetricsBatch>) -> StatusCode {
    state.buffer.send_metrics(b.series, b.points);
    StatusCode::NO_CONTENT
}
```

- [ ] **Step 2: Update `main()` to construct `WriteBuffer` from env vars**

In `services/storage-writer/src/main.rs`, update the imports at the top to include `buffer::WriteBuffer`:

```rust
use storage_writer::{AppState, buffer::WriteBuffer, observability};
```

In `main()`, after the ClickHouse client is built and before `tokio::spawn(retention::start_retention_worker(...))`, add:

```rust
    let flush_max_rows: usize = std::env::var("STORAGE_WRITER_FLUSH_MAX_ROWS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5_000);
    let flush_interval = std::time::Duration::from_millis(
        std::env::var("STORAGE_WRITER_FLUSH_INTERVAL_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(500),
    );
    let buffer = std::sync::Arc::new(WriteBuffer::new(ch.clone(), flush_max_rows, flush_interval));
```

Then update the `AppState` construction to include `buffer`:

Old:
```rust
    let state = AppState {
        ch,
        metrics: Arc::new(observability::StorageWriterMetrics::new()),
    };
```

New:
```rust
    let state = AppState {
        buffer,
        ch,
        metrics: Arc::new(observability::StorageWriterMetrics::new()),
    };
```

- [ ] **Step 3: Remove unused imports in `main.rs`**

The old handlers referenced `spans::insert_spans`, `logs::insert_logs`, `metrics::insert_metric_series`, `metrics::insert_metric_points` directly. After the change, those local references are gone. Remove any now-unused `use` statements or `mod` declarations that may have been added. (The `mod retention;` declaration stays.)

Run:
```bash
cargo build -p storage-writer 2>&1 | grep "^warning.*unused"
```

Remove any unused import warnings.

- [ ] **Step 4: Build and run all tests**

```bash
cargo build -p storage-writer
cargo test -p storage-writer --lib
```

Expected: clean build, all tests pass.

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add services/storage-writer/src/main.rs
git commit -m "feat(storage-writer): wire WriteBuffer into handlers; add FLUSH_MAX_ROWS and FLUSH_INTERVAL_MS config"
```

---

## Task 4: Smoke test + roadmap update

**Files:**
- Modify: `tests/e2e/smoke_test_unit.sh`
- Modify: `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`
- Modify: `docs/agent-context.md`

- [ ] **Step 1: Add buffer existence assertion to `smoke_test_unit.sh`**

In `tests/e2e/smoke_test_unit.sh`, after the existing `test_storage_writer_uses_telemetry_constant` function and its `run_test` registration, add:

```bash
test_storage_writer_has_write_buffer() {
  local buf="$SCRIPT_DIR/../../services/storage-writer/src/buffer.rs"

  if [ ! -f "$buf" ]; then
    echo "FAIL: services/storage-writer/src/buffer.rs must exist (async write buffer for ClickHouse insert efficiency)"
    exit 1
  fi
}
```

And register it at the bottom alongside the existing run_test lines:

```bash
run_test "storage-writer has write buffer" test_storage_writer_has_write_buffer
```

- [ ] **Step 2: Run `smoke_test_unit.sh` to verify all tests pass**

```bash
bash tests/e2e/smoke_test_unit.sh
```

Expected: all tests pass including the new one.

- [ ] **Step 3: Mark ClickHouse Insert Efficiency complete in the roadmap**

In `docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md`, find:

```markdown
- [ ] **ClickHouse Insert Efficiency**: Align `stream-processor` batching with `storage-writer` to ensure ClickHouse receives large, efficient blocks rather than many small inserts.
```

Replace with:

```markdown
- [x] **ClickHouse Insert Efficiency**: Align `stream-processor` batching with `storage-writer` to ensure ClickHouse receives large, efficient blocks rather than many small inserts. (COMPLETED 2026-05-30) `storage-writer` gains an async `WriteBuffer` (`src/buffer.rs`) with three `tokio::mpsc` channels (spans, logs, metrics). HTTP handlers push to channels and return 204 immediately; background flush tasks fire on `STORAGE_WRITER_FLUSH_MAX_ROWS` (default 5 000) or `STORAGE_WRITER_FLUSH_INTERVAL_MS` (default 500 ms). Unit tests cover count- and timeout-triggered flushes.
```

- [ ] **Step 4: Update `docs/agent-context.md`**

Find the active plan line:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, and P4-S4 dashboard ReBAC complete. Next: P4-S3b SCIM/SSO (if required by v1 customers) or P4-S5+ Phase 5 work.
```

Replace with:

```
- Active detailed implementation plan: none — RF-2, RF-3, RF-6, P4-S9, stream-processor batching, Telemetry Loop Prevention, P4-S4 dashboard ReBAC, and ClickHouse insert efficiency complete. Next: P4-S3b SCIM/SSO (if required by v1 customers), Context Preservation (frontend), or Live Tail.
```

- [ ] **Step 5: Format and commit**

```bash
cargo fmt --all
git add tests/e2e/smoke_test_unit.sh \
        docs/superpowers/plans/2026-05-07-remaining-roadmap-plan.md \
        docs/agent-context.md
git commit -m "chore(telemetry): add smoke test for write buffer; mark ClickHouse insert efficiency complete"
```

---

## Verification Checklist (run before pushing)

- [ ] `cargo test -p storage-writer --lib` — all tests pass including `flush_on_count` and `flush_on_timeout`
- [ ] `cargo build --workspace` — clean build
- [ ] `cargo fmt --all -- --check` — no formatting issues
- [ ] `cargo clippy -p storage-writer --all-targets -- -D warnings` — no warnings
- [ ] `bash tests/e2e/smoke_test_unit.sh` — all 18 tests pass
