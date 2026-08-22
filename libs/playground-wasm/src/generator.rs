//! Deterministic, seeded demo trace generator for the browser-local
//! playground. Scoped-down Phase 4 slice: traces only (2-3 span nested
//! traces across a small fixed topology), not the full plan section 8
//! generator (logs/metrics/deployments/SLO burn are future work). See
//! `docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md`.

use serde::Serialize;

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct GeneratedSpan {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: String,
    pub service_name: String,
    pub operation_name: String,
    /// Nanoseconds, as a decimal string — avoids JS `Number` precision loss
    /// for large integers (same convention `NlqIr.time_range` already uses
    /// across the JS/Rust boundary elsewhere in this codebase, per ADR-030).
    pub duration_ns: String,
    pub status_code: String,
    pub environment: String,
    pub start_time_unix_nano: String,
}

/// Default topology (plan section 8): `web -> api-gateway -> checkout ->
/// payment -> inventory -> database`, `api-gateway -> catalog / search`.
const TOPOLOGY: &[(&str, &[&str])] = &[
    ("web", &["api-gateway"]),
    ("api-gateway", &["checkout", "catalog", "search"]),
    ("checkout", &["payment"]),
    ("payment", &["inventory"]),
    ("inventory", &["database"]),
    ("catalog", &[]),
    ("search", &[]),
    ("database", &[]),
];

const ROOT_SERVICES: &[&str] = &["web", "api-gateway"];
const TRACE_COUNT: u32 = 40;
const WINDOW_SECS: u32 = 55 * 60;

/// Tiny hand-rolled xorshift32 PRNG — deterministic given a seed, no crate
/// dependency (avoids repeating the `uuid`/`getrandom`-on-wasm32 friction
/// from the query-core extraction; this generator never needs OS entropy).
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Rng(if seed == 0 { 1 } else { seed })
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    fn next_range(&mut self, max: u32) -> u32 {
        if max == 0 { 0 } else { self.next_u32() % max }
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u32() as f64) / (u32::MAX as f64)
    }
}

fn children_of(service: &str) -> &'static [&'static str] {
    TOPOLOGY
        .iter()
        .find(|(s, _)| *s == service)
        .map(|(_, c)| *c)
        .unwrap_or(&[])
}

fn operation_name(service: &str) -> &'static str {
    match service {
        "web" => "GET /",
        "api-gateway" => "GET /api/route",
        "checkout" => "POST /checkout",
        "payment" => "POST /charge",
        "inventory" => "GET /inventory/check",
        "database" => "QUERY spans",
        "catalog" => "GET /catalog",
        "search" => "GET /search",
        _ => "UNKNOWN",
    }
}

fn error_weight(service: &str) -> f64 {
    match service {
        "payment" => 0.15,
        "inventory" => 0.08,
        _ => 0.04,
    }
}

fn base_duration_ns(rng: &mut Rng, service: &str) -> i64 {
    let (min_ms, max_ms): (i64, i64) = match service {
        "payment" => (50, 500),
        "database" => (2, 40),
        "inventory" => (10, 120),
        _ => (5, 80),
    };
    let ms = min_ms + rng.next_range((max_ms - min_ms) as u32) as i64;
    ms * 1_000_000
}

fn roll_status(rng: &mut Rng, service: &str) -> &'static str {
    if rng.next_f64() < error_weight(service) {
        "ERROR"
    } else {
        "OK"
    }
}

/// Generates ~`TRACE_COUNT` traces (2-3 spans each) spread over the last
/// `WINDOW_SECS` relative to `now_unix_nano`. Deterministic: the same
/// `(seed, now_unix_nano)` pair always produces identical output.
pub fn generate_spans(seed: u32, now_unix_nano: i64) -> Vec<GeneratedSpan> {
    let mut rng = Rng::new(seed);
    let mut spans = Vec::new();

    for i in 0..TRACE_COUNT {
        let trace_id = format!("playground-trace-{i}");
        let root_service = ROOT_SERVICES[rng.next_range(ROOT_SERVICES.len() as u32) as usize];
        let offset_ns = rng.next_range(WINDOW_SECS) as i64 * 1_000_000_000;
        let root_start = now_unix_nano - offset_ns;
        let environment = if rng.next_f64() < 0.25 {
            "staging"
        } else {
            "production"
        };

        let root_span_id = format!("playground-span-{i}-0");
        let root_duration = base_duration_ns(&mut rng, root_service);
        let root_status = roll_status(&mut rng, root_service);

        spans.push(GeneratedSpan {
            trace_id: trace_id.clone(),
            span_id: root_span_id.clone(),
            parent_span_id: String::new(),
            service_name: root_service.to_string(),
            operation_name: operation_name(root_service).to_string(),
            duration_ns: root_duration.to_string(),
            status_code: root_status.to_string(),
            environment: environment.to_string(),
            start_time_unix_nano: root_start.to_string(),
        });

        let children = children_of(root_service);
        if children.is_empty() {
            continue;
        }
        let child_service = children[rng.next_range(children.len() as u32) as usize];
        let child_start = root_start + 1_000_000;
        let child_duration = base_duration_ns(&mut rng, child_service)
            .min((root_duration - 1_000_000).max(1_000_000));
        let child_status = roll_status(&mut rng, child_service);
        let child_span_id = format!("playground-span-{i}-1");

        spans.push(GeneratedSpan {
            trace_id: trace_id.clone(),
            span_id: child_span_id.clone(),
            parent_span_id: root_span_id,
            service_name: child_service.to_string(),
            operation_name: operation_name(child_service).to_string(),
            duration_ns: child_duration.to_string(),
            status_code: child_status.to_string(),
            environment: environment.to_string(),
            start_time_unix_nano: child_start.to_string(),
        });

        if rng.next_f64() < 0.15 {
            let grandchildren = children_of(child_service);
            if !grandchildren.is_empty() {
                let gc_service = grandchildren[rng.next_range(grandchildren.len() as u32) as usize];
                let gc_start = child_start + 1_000_000;
                let gc_duration = base_duration_ns(&mut rng, gc_service)
                    .min((child_duration - 1_000_000).max(1_000_000));
                let gc_status = roll_status(&mut rng, gc_service);

                spans.push(GeneratedSpan {
                    trace_id,
                    span_id: format!("playground-span-{i}-2"),
                    parent_span_id: child_span_id,
                    service_name: gc_service.to_string(),
                    operation_name: operation_name(gc_service).to_string(),
                    duration_ns: gc_duration.to_string(),
                    status_code: gc_status.to_string(),
                    environment: environment.to_string(),
                    start_time_unix_nano: gc_start.to_string(),
                });
            }
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    const NOW: i64 = 1_700_003_600_000_000_000;

    #[test]
    fn same_seed_produces_identical_output() {
        let a = generate_spans(42, NOW);
        let b = generate_spans(42, NOW);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_produce_different_output() {
        let a = generate_spans(1, NOW);
        let b = generate_spans(2, NOW);
        assert_ne!(a, b);
    }

    #[test]
    fn generates_at_least_one_span_per_trace() {
        let spans = generate_spans(7, NOW);
        assert!(spans.len() >= 40);
    }

    #[test]
    fn root_spans_have_empty_parent_id() {
        let spans = generate_spans(7, NOW);
        let roots: Vec<_> = spans.iter().filter(|s| s.span_id.ends_with("-0")).collect();
        assert_eq!(roots.len(), 40);
        for root in roots {
            assert_eq!(root.parent_span_id, "");
        }
    }

    #[test]
    fn child_spans_start_after_and_end_before_parent() {
        let spans = generate_spans(7, NOW);
        let by_span_id: HashMap<&str, &GeneratedSpan> =
            spans.iter().map(|s| (s.span_id.as_str(), s)).collect();

        for span in &spans {
            if span.parent_span_id.is_empty() {
                continue;
            }
            let parent = by_span_id
                .get(span.parent_span_id.as_str())
                .expect("parent span must exist");
            let parent_start: i64 = parent.start_time_unix_nano.parse().unwrap();
            let parent_duration: i64 = parent.duration_ns.parse().unwrap();
            let child_start: i64 = span.start_time_unix_nano.parse().unwrap();
            let child_duration: i64 = span.duration_ns.parse().unwrap();

            assert!(
                child_start >= parent_start,
                "child must start at or after parent"
            );
            assert!(
                child_start + child_duration <= parent_start + parent_duration,
                "child must end at or before parent ends"
            );
        }
    }

    #[test]
    fn every_span_pair_is_topology_consistent() {
        let spans = generate_spans(7, NOW);
        let by_span_id: HashMap<&str, &GeneratedSpan> =
            spans.iter().map(|s| (s.span_id.as_str(), s)).collect();

        for span in &spans {
            if span.parent_span_id.is_empty() {
                continue;
            }
            let parent = by_span_id.get(span.parent_span_id.as_str()).unwrap();
            let allowed = children_of(&parent.service_name);
            assert!(
                allowed.contains(&span.service_name.as_str()),
                "{} is not a valid downstream of {}",
                span.service_name,
                parent.service_name
            );
        }
    }

    #[test]
    fn start_times_are_within_the_configured_window() {
        let spans = generate_spans(7, NOW);
        let window_ns = (WINDOW_SECS as i64 + 5) * 1_000_000_000; // small margin for child offsets
        for span in &spans {
            let start: i64 = span.start_time_unix_nano.parse().unwrap();
            assert!(start <= NOW);
            assert!(NOW - start <= window_ns);
        }
    }
}
