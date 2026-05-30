# ClickHouse Insert Efficiency Design

**Date:** 2026-05-30
**Status:** Approved for implementation

---

## Goal

Aggregate small HTTP batches arriving at `storage-writer` from `stream-processor` before writing to ClickHouse. Currently `storage-writer` issues one ClickHouse INSERT per HTTP call; in low-traffic windows, stream-processor's 200 ms timeout flushes produce batches of a handful of rows, creating many tiny inserts. ClickHouse performs best with inserts of 1 000+ rows.

---

## Context

`stream-processor` already batches up to 500 envelopes per flush (`STREAM_PROCESSOR_BATCH_SIZE`, `STREAM_PROCESSOR_BATCH_INTERVAL_MS`). In high-traffic scenarios this produces large HTTP payloads and efficient ClickHouse inserts. In low-traffic scenarios (dev, overnight, canary) the timer fires with only a few rows.

`storage-writer` currently calls `insert_spans`, `insert_logs`, `insert_metric_series`, `insert_metric_points` synchronously and returns 204. Each function issues one ClickHouse HTTP INSERT.

The fix: add an async `WriteBuffer` in `storage-writer` that accumulates rows across HTTP calls before flushing to ClickHouse.

---

## Durability Trade-off

The buffer is **async (fire-and-forget)**:
- HTTP handler pushes rows into a bounded channel and returns 204 immediately.
- ClickHouse insert happens in a background task.
- A `storage-writer` crash between 204 and the flush loses that window of data. The corresponding Kafka offsets have already been committed by `stream-processor`.

This is acceptable for an observability platform where best-effort delivery is the norm. The crash window is bounded by `STORAGE_WRITER_FLUSH_INTERVAL_MS` (default 500 ms).

---

## Architecture

### New file: `services/storage-writer/src/buffer.rs`

A `WriteBuffer` struct owns one `tokio::mpsc` channel per logical flush group and spawns one background flush task per group.

**Flush groups:**

| Group | Signals | Channel item |
|---|---|---|
| `spans` | `Span` rows | `Vec<Span>` |
| `logs` | `LogRecord` rows | `Vec<LogRecord>` |
| `metrics` | `MetricSeries` + `MetricPoint` rows | `(Vec<MetricSeries>, Vec<MetricPoint>)` |

Span events are extracted from spans inside the existing `insert_spans` function — no separate channel needed.

**Flush triggers (OR condition):**
- Accumulated row count ≥ `max_rows` (`STORAGE_WRITER_FLUSH_MAX_ROWS`, default 5 000)
- No new message received within `flush_interval` (`STORAGE_WRITER_FLUSH_INTERVAL_MS`, default 500 ms)

**Back-pressure:** channel capacity = 2 × `max_rows` (in terms of items, not rows). If the channel is full when a handler tries to send, the send is dropped and an error is logged. This prevents unbounded memory growth when ClickHouse is slow.

**Public interface:**

```rust
pub struct WriteBuffer { /* opaque */ }

impl WriteBuffer {
    pub fn new(ch: clickhouse::Client, max_rows: usize, flush_interval: Duration) -> Self

    /// Non-blocking. Drops the batch and logs an error if the channel is full.
    pub fn send_spans(&self, spans: Vec<domain::Span>)
    pub fn send_logs(&self, logs: Vec<domain::LogRecord>)
    pub fn send_metrics(&self, series: Vec<domain::MetricSeries>, points: Vec<domain::MetricPoint>)
}
```

### Modified: `services/storage-writer/src/main.rs`

- `AppState` gains a `buffer: Arc<WriteBuffer>` field (replacing the direct `ch: Client` used by write handlers).
- `write_spans`, `write_logs`, `write_metrics` handlers call `state.buffer.send_*` and return 204 immediately.
- `main()` constructs `WriteBuffer` from env vars and injects into `AppState`.
- The ClickHouse client is still stored in `AppState` for the retention worker (which is not batched).

### Unchanged

- `src/spans.rs`, `src/logs.rs`, `src/metrics.rs` — insert functions remain; called by the flush task.
- `src/retention.rs` — retention worker uses `ch` directly, not the buffer.
- `src/observability.rs` — metrics endpoint unchanged.

---

## Flush Task Implementation

Each flush task is a `tokio::spawn`ed loop:

```
loop:
  match timeout(flush_interval, rx.recv()):
    Ok(Some(batch)) => accumulate into local vec; if count >= max_rows: flush
    Ok(None)        => channel closed, flush remainder and exit
    Err(timeout)    => if vec non-empty: flush
```

On flush error (ClickHouse unavailable): log the error, drop the batch, continue. Do not retry — the next flush cycle will attempt a fresh insert.

---

## Configuration

| Env var | Default | Description |
|---|---|---|
| `STORAGE_WRITER_FLUSH_MAX_ROWS` | `5000` | Flush when accumulated rows reach this count |
| `STORAGE_WRITER_FLUSH_INTERVAL_MS` | `500` | Flush after this many ms with no new data |

Both vars apply uniformly to all three flush groups.

---

## Testing

### Unit tests in `src/buffer.rs`

1. **Flush on count**: send `max_rows + 1` items in two batches; assert `insert_*` called once with all rows combined.
2. **Flush on timeout**: send a partial batch; advance mocked time past `flush_interval`; assert `insert_*` called with the partial batch.
3. **Back-pressure drop**: fill channel to capacity; next `send_*` does not block and logs an error.

Testing strategy: `WriteBuffer` exposes a `#[cfg(test)]` constructor `WriteBuffer::with_flush_fn` that accepts a boxed async closure instead of a `clickhouse::Client`. This allows unit tests to capture flushed batches without a running ClickHouse. The production constructor `WriteBuffer::new` uses the existing `insert_spans`, `insert_logs`, etc. functions directly.

### Integration tests

Existing `spans.rs`, `logs.rs`, `metrics.rs` integration tests exercise the insert functions directly — no changes needed.

### Smoke test

Add an assertion to `tests/e2e/smoke_test_unit.sh` that `services/storage-writer/src/buffer.rs` exists, guarding against accidental deletion.

---

## Backward Compatibility

- All existing HTTP API contracts (`/internal/spans`, `/internal/logs`, `/internal/metrics`) are unchanged.
- Response codes (204, 500) are unchanged; 500 can no longer be returned for ClickHouse errors (the handler doesn't wait), which is a semantic change but acceptable — HTTP 500 was unreliable anyway since ClickHouse errors are async in practice.
- Docker Compose and Helm: two new optional env vars added to `stream-processor` and `docker-compose.yml` / Helm values.
