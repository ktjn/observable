//! Query semantic planning now lives in `libs/query-core` (portable, no
//! server-adapter dependencies) — see
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`
//! section 6. Re-exported here so existing call sites are unaffected.

pub use query_core::planner::QueryPlanner;
