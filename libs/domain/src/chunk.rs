//! Size-bounded serialization for queue messages.
//!
//! Redpanda (`max.message.bytes`, 1 MiB default) and librdkafka
//! (`message.max.bytes`, 1,000,000 default) reject larger records. One OTLP
//! export of a few hundred spans or log records serializes past that, so
//! producers must split a payload into several records instead of sending it
//! whole.

use serde::Serialize;

use crate::envelope::{EnvelopePayload, NormalizedTelemetryBatch, TelemetryEnvelope};

/// Byte budget for one queue record value. Leaves headroom under librdkafka's
/// 1,000,000-byte `message.max.bytes` for key, headers and batch framing.
pub const MAX_QUEUE_MESSAGE_BYTES: usize = 900_000;

/// A value that can be halved into two smaller values of the same type.
pub trait Splittable: Sized {
    /// Splits into two non-empty halves, or `None` when it holds at most one item.
    fn split(self) -> Option<(Self, Self)>;
}

/// Serializes `value` as JSON into one or more chunks of at most `max_bytes`,
/// halving it until each part fits. Fails if a single item exceeds `max_bytes`.
pub fn serialize_chunked<T: Serialize + Splittable>(
    value: T,
    max_bytes: usize,
) -> Result<Vec<Vec<u8>>, ChunkError> {
    let mut out = Vec::new();
    let mut stack = vec![value];
    while let Some(v) = stack.pop() {
        let bytes = serde_json::to_vec(&v).map_err(ChunkError::Serialize)?;
        if bytes.len() <= max_bytes {
            out.push(bytes);
            continue;
        }
        match v.split() {
            // Push the second half first so chunks keep the original order.
            Some((a, b)) => {
                stack.push(b);
                stack.push(a);
            }
            None => {
                return Err(ChunkError::ItemTooLarge {
                    size: bytes.len(),
                    max: max_bytes,
                });
            }
        }
    }
    Ok(out)
}

#[derive(Debug)]
pub enum ChunkError {
    Serialize(serde_json::Error),
    ItemTooLarge { size: usize, max: usize },
}

impl std::fmt::Display for ChunkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "serialize: {e}"),
            Self::ItemTooLarge { size, max } => {
                write!(f, "single telemetry item is {size} bytes, limit {max}")
            }
        }
    }
}

impl std::error::Error for ChunkError {}

fn halve<T>(mut v: Vec<T>) -> (Vec<T>, Vec<T>) {
    let tail = v.split_off(v.len() / 2);
    (v, tail)
}

impl Splittable for EnvelopePayload {
    fn split(self) -> Option<(Self, Self)> {
        match self {
            Self::Spans(s) if s.len() > 1 => {
                let (a, b) = halve(s);
                Some((Self::Spans(a), Self::Spans(b)))
            }
            Self::Logs(l) if l.len() > 1 => {
                let (a, b) = halve(l);
                Some((Self::Logs(a), Self::Logs(b)))
            }
            // Series and points are written to separate tables; splitting
            // them independently is safe.
            Self::Metrics { series, points } if series.len() + points.len() > 1 => {
                let (pa, pb) = halve(points);
                let (sa, sb) = halve(series);
                let a = Self::Metrics {
                    series: sa,
                    points: pa,
                };
                let b = Self::Metrics {
                    series: sb,
                    points: pb,
                };
                Some((a, b))
            }
            _ => None,
        }
    }
}

impl Splittable for TelemetryEnvelope {
    fn split(self) -> Option<(Self, Self)> {
        let Self {
            envelope_id,
            tenant_id,
            environment,
            received_at_unix_nano,
            payload,
        } = self;
        let (pa, pb) = payload.split()?;
        let b = Self {
            envelope_id: uuid::Uuid::new_v4(),
            tenant_id,
            environment: environment.clone(),
            received_at_unix_nano,
            payload: pb,
        };
        let a = Self {
            envelope_id,
            tenant_id,
            environment,
            received_at_unix_nano,
            payload: pa,
        };
        Some((a, b))
    }
}

impl Splittable for NormalizedTelemetryBatch {
    fn split(self) -> Option<(Self, Self)> {
        let total = self.spans.len() + self.logs.len() + self.series.len() + self.points.len();
        if total <= 1 {
            return None;
        }
        let (sa, sb) = halve(self.spans);
        let (la, lb) = halve(self.logs);
        let (ra, rb) = halve(self.series);
        let (pa, pb) = halve(self.points);
        Some((
            Self {
                spans: sa,
                logs: la,
                series: ra,
                points: pa,
            },
            Self {
                spans: sb,
                logs: lb,
                series: rb,
                points: pb,
            },
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LogRecord, Span};
    use std::collections::HashMap;
    use uuid::Uuid;

    fn big_span(i: usize) -> Span {
        Span {
            span_id: format!("{i:016x}"),
            attributes: HashMap::from([("pad".into(), serde_json::json!("x".repeat(1_000)))]),
            ..Default::default()
        }
    }

    fn envelope(n: usize) -> TelemetryEnvelope {
        TelemetryEnvelope {
            envelope_id: Uuid::new_v4(),
            tenant_id: Uuid::new_v4(),
            environment: "otel-demo".into(),
            received_at_unix_nano: 1,
            payload: EnvelopePayload::Spans((0..n).map(big_span).collect()),
        }
    }

    fn span_ids(chunks: &[Vec<u8>]) -> Vec<String> {
        chunks
            .iter()
            .flat_map(|c| {
                let e: TelemetryEnvelope = serde_json::from_slice(c).unwrap();
                match e.payload {
                    EnvelopePayload::Spans(s) => s.into_iter().map(|s| s.span_id),
                    _ => panic!("expected spans"),
                }
            })
            .collect()
    }

    #[test]
    fn small_envelope_is_one_chunk() {
        let chunks = serialize_chunked(envelope(3), MAX_QUEUE_MESSAGE_BYTES).unwrap();
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn large_envelope_splits_under_limit_and_keeps_every_span_in_order() {
        let env = envelope(2_000);
        let tenant = env.tenant_id;
        assert!(serde_json::to_vec(&env).unwrap().len() > MAX_QUEUE_MESSAGE_BYTES);
        let chunks = serialize_chunked(env, MAX_QUEUE_MESSAGE_BYTES).unwrap();
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|c| c.len() <= MAX_QUEUE_MESSAGE_BYTES));
        let want: Vec<String> = (0..2_000).map(|i| format!("{i:016x}")).collect();
        assert_eq!(span_ids(&chunks), want);
        for c in &chunks {
            let e: TelemetryEnvelope = serde_json::from_slice(c).unwrap();
            assert_eq!(e.tenant_id, tenant);
            assert_eq!(e.environment, "otel-demo");
        }
    }

    #[test]
    fn single_oversized_item_is_an_error() {
        let err = serialize_chunked(envelope(1), 100).unwrap_err();
        assert!(matches!(err, ChunkError::ItemTooLarge { .. }));
    }

    #[test]
    fn normalized_batch_splits_all_signal_vectors() {
        let batch = NormalizedTelemetryBatch {
            spans: (0..600).map(big_span).collect(),
            logs: (0..600)
                .map(|_| LogRecord {
                    body: serde_json::json!("y".repeat(1_000)),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let chunks = serialize_chunked(batch, MAX_QUEUE_MESSAGE_BYTES).unwrap();
        assert!(chunks.len() > 1);
        let (mut spans, mut logs) = (0, 0);
        for c in &chunks {
            assert!(c.len() <= MAX_QUEUE_MESSAGE_BYTES);
            let b: NormalizedTelemetryBatch = serde_json::from_slice(c).unwrap();
            spans += b.spans.len();
            logs += b.logs.len();
        }
        assert_eq!((spans, logs), (600, 600));
    }
}
