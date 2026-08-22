//! Platform-neutral domain types with no server-adapter dependencies —
//! compiles natively and to `wasm32-unknown-unknown`. See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 6.

pub mod nlq;

pub use nlq::{
    NlqFilter, NlqFilterOp, NlqIr, NlqOperation, NlqSignal, NlqTimeRange, NlqVisualizationHint,
};
