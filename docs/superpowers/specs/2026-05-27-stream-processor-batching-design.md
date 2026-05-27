# Stream Processor Batching Design

**Date:** 2026-05-27
**Status:** Approved

## Goal

Eliminate the 1-HTTP-POST-per-envelope bottleneck in `stream-processor` by collecting multiple Kafka messages into a batch before flushing to `storage-writer`. Under high load this reduces connection overhead and gives `storage-writer` larger, more efficient ClickHouse inserts.

## Architecture

A new `run_batch` method on `QueueConsumer` accumulates deserialized `TelemetryEnvelope` values and calls a batch handler when either a count or time threshold is reached. `main.rs` merges the batch by payload type and fires at most three HTTP POSTs per flush (spans, logs, metrics).

## Consumer API

`consumer.rs` gains `run_batch` alongside (and replacing) `run`:

```rust
pub async fn run_batch<F, Fut>(
    &self,
    max_size: usize,
    max_wait: Duration,
    mut handler: F,
) -> anyhow::Result<()>
where
    F: FnMut(Vec<TelemetryEnvelope>) -> Fut,
    Fut: Future<Output = anyhow::Result<()>>,
```

Internally a `tokio::select!` loop accumulates into a `Vec<TelemetryEnvelope>`. It flushes when the vec reaches `max_size` **or** the interval timer fires — whichever comes first. After each flush the buffer is cleared and the timer resets. Deserialization errors are logged and skipped. `run` is removed; `main.rs` is its only caller.

## Batch Handler (main.rs)

The handler receives `Vec<TelemetryEnvelope>` and merges by payload type before posting:

```
for env in batch:
    Spans   → record span metrics + normalise → all_spans
    Logs    → normalise → all_logs
    Metrics → normalise → all_series / all_points

if !all_spans.is_empty()   → POST /internal/spans
if !all_logs.is_empty()    → POST /internal/logs
if !all_series.is_empty()  → POST /internal/metrics
```

**Mixed-environment batches:** if any envelope in the batch is non-observable, inject trace context and set `x-observable-environment` to the first non-observable environment seen. If all are observable, suppress tracing as before.

## Configuration

| Env var | Default | Meaning |
|---|---|---|
| `STREAM_PROCESSOR_BATCH_SIZE` | `500` | Max envelopes per flush |
| `STREAM_PROCESSOR_BATCH_INTERVAL_MS` | `200` | Max ms before flush |

Read in `main.rs` with `std::env::var(...).unwrap_or_else(...)`. No Helm/Compose changes required — defaults are production-safe.

## Error Handling

If a POST fails, the handler returns `Err`. `run_batch` logs the error with `tracing::warn!` and continues to the next batch. Envelopes in a failed batch are dropped; Kafka offsets are committed so the consumer does not stall. Retry and dead-letter queue are out of scope for this slice.

## Testing

**Unit tests in `consumer.rs`** — test `run_batch` accumulation logic in isolation:
1. Flush on count — push `max_size` messages, verify handler called once with all of them.
2. Flush on timeout — push fewer than `max_size` messages, advance time past `max_wait` with `tokio::time::pause()` / `advance()`, verify handler called with the partial batch.

**Integration test in `services/stream-processor/tests/`** — construct a `Vec<TelemetryEnvelope>` with mixed spans + logs + metrics, call the batch handler directly, verify correct per-type merge output. No real Kafka required.

The existing `redpanda_integration.rs` (`#[ignore]`) is unchanged.

## Files Changed

| File | Change |
|---|---|
| `services/stream-processor/src/consumer.rs` | Replace `run` with `run_batch` |
| `services/stream-processor/src/main.rs` | Read batch config env vars; switch to `run_batch` with merging handler |
| `services/stream-processor/tests/batch_handler_integration.rs` | New integration test |

## Out of Scope

- Retry or dead-letter queue on flush failure
- Per-tenant batching or routing
- Changes to `storage-writer` endpoints
- Helm / Docker Compose env var additions (defaults are sufficient)
