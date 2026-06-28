// Generated artifacts for the `tracing` domain (models/tracing.mdl).
// Regenerate with:
//   modelable compile models --target rust --out <tmp>
// then copy tracing.SpanRow.v1.rs / tracing.SpanEventRow.v1.rs / tracing.Span.v1.rs /
// tracing.SpanEvent.v1.rs from <tmp>/tracing/ into this directory, renaming to
// snake_case file names. Do not hand-edit the generated files themselves.
#![allow(dead_code, unused_imports, clippy::useless_conversion)]

#[cfg(feature = "storage")]
mod tracing_span_event_row_v1;
mod tracing_span_event_v1;
#[cfg(feature = "storage")]
mod tracing_span_row_v1;
mod tracing_span_v1;

#[cfg(feature = "storage")]
pub(crate) use tracing_span_event_row_v1::TracingSpanEventRowV1;
#[cfg(feature = "storage")]
pub(crate) use tracing_span_row_v1::TracingSpanRowV1;
#[cfg(feature = "storage")]
pub(crate) use tracing_span_row_v1::{TracingSpanRowV1SpanKind, TracingSpanRowV1StatusCode};
