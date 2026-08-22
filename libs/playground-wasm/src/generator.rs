//! Deterministic, seeded demo trace/log/metric generator for the
//! browser-local playground. Scoped-down Phase 4 slice: traces, one log per
//! span, and a small fixed metric catalog (deployments/SLO burn are future
//! work). See
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

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct GeneratedLog {
    pub log_id: String,
    /// Nanoseconds, as a decimal string — see `GeneratedSpan::duration_ns`.
    pub timestamp_unix_nano: String,
    pub observed_timestamp_unix_nano: String,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub trace_id: String,
    pub span_id: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
}

fn log_body(service: &str, status: &str) -> String {
    if status == "ERROR" {
        format!("{service} request failed")
    } else {
        format!("{service} request completed")
    }
}

/// Derives one log record per generated span (same seed/topology/timing),
/// so trace_id/span_id correlation between the Traces and Logs pages is
/// real, not coincidental. Spans with `status_code == "ERROR"` produce an
/// ERROR-severity log; otherwise INFO, with a small chance of WARN.
pub fn generate_logs(seed: u32, now_unix_nano: i64) -> Vec<GeneratedLog> {
    let spans = generate_spans(seed, now_unix_nano);
    // Offset from the span-generation seed so severity/warn rolls don't
    // shadow the span RNG's own draws.
    let mut rng = Rng::new(seed.wrapping_add(0x9E37_79B9));

    spans
        .into_iter()
        .map(|span| {
            let (severity_number, severity_text) = if span.status_code == "ERROR" {
                (17, "ERROR")
            } else if rng.next_f64() < 0.1 {
                (13, "WARN")
            } else {
                (9, "INFO")
            };

            GeneratedLog {
                log_id: format!("playground-log-{}", span.span_id),
                timestamp_unix_nano: span.start_time_unix_nano.clone(),
                observed_timestamp_unix_nano: span.start_time_unix_nano,
                severity_number,
                severity_text: severity_text.to_string(),
                body: log_body(&span.service_name, &span.status_code),
                trace_id: span.trace_id,
                span_id: span.span_id,
                service_name: span.service_name,
                environment: span.environment,
                host_id: "playground-host".to_string(),
            }
        })
        .collect()
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct GeneratedMetricSeries {
    pub metric_series_id: String,
    pub metric_name: String,
    pub description: String,
    pub unit: String,
    pub metric_type: String,
    pub is_monotonic: bool,
    pub aggregation_temporality: String,
    pub service_name: String,
    pub environment: String,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct GeneratedMetricPoint {
    pub metric_series_id: String,
    /// Nanoseconds, as a decimal string — see `GeneratedSpan::duration_ns`.
    pub time_unix_nano: String,
    pub start_time_unix_nano: String,
    pub value_double: f64,
}

/// `(name, type, unit, is_monotonic, min_value, max_value)`. Values are
/// plausible ranges for a demo graph, not a real OTel cumulative-counter
/// simulation — `http.server.request_count` here is a modest scope-down
/// (per-point value, not a running total).
const METRIC_DEFS: &[(&str, &str, &str, bool, f64, f64)] = &[
    ("http.server.request_count", "sum", "1", true, 10.0, 200.0),
    ("http.server.duration", "gauge", "ms", false, 5.0, 500.0),
    ("system.cpu.utilization", "gauge", "1", false, 0.05, 0.95),
];

const METRIC_POINT_COUNT: u32 = 20;

/// Generates a small fixed metric catalog (`METRIC_DEFS` x every topology
/// service) with `METRIC_POINT_COUNT` points each, spread evenly over
/// `WINDOW_SECS` relative to `now_unix_nano`. Deterministic like
/// `generate_spans`/`generate_logs`.
pub fn generate_metrics(
    seed: u32,
    now_unix_nano: i64,
) -> (Vec<GeneratedMetricSeries>, Vec<GeneratedMetricPoint>) {
    let mut rng = Rng::new(seed.wrapping_add(0x1234_5678));
    let mut series = Vec::new();
    let mut points = Vec::new();
    let interval_ns = (WINDOW_SECS as i64 * 1_000_000_000) / METRIC_POINT_COUNT as i64;

    let mut idx = 0u32;
    for (service_name, _) in TOPOLOGY {
        for (metric_name, metric_type, unit, is_monotonic, min_v, max_v) in METRIC_DEFS {
            let series_id = format!("playground-metric-{idx}");
            series.push(GeneratedMetricSeries {
                metric_series_id: series_id.clone(),
                metric_name: metric_name.to_string(),
                description: format!("{metric_name} for {service_name}"),
                unit: unit.to_string(),
                metric_type: metric_type.to_string(),
                is_monotonic: *is_monotonic,
                aggregation_temporality: "cumulative".to_string(),
                service_name: service_name.to_string(),
                environment: "production".to_string(),
            });

            for p in 0..METRIC_POINT_COUNT {
                let t = now_unix_nano - (METRIC_POINT_COUNT - 1 - p) as i64 * interval_ns;
                let value = min_v + rng.next_f64() * (max_v - min_v);
                points.push(GeneratedMetricPoint {
                    metric_series_id: series_id.clone(),
                    time_unix_nano: t.to_string(),
                    start_time_unix_nano: t.to_string(),
                    value_double: value,
                });
            }
            idx += 1;
        }
    }

    (series, points)
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

    // ── logs ──────────────────────────────────────────────────────────────

    #[test]
    fn logs_same_seed_produces_identical_output() {
        let a = generate_logs(42, NOW);
        let b = generate_logs(42, NOW);
        assert_eq!(a, b);
    }

    #[test]
    fn one_log_per_span() {
        let spans = generate_spans(7, NOW);
        let logs = generate_logs(7, NOW);
        assert_eq!(spans.len(), logs.len());
    }

    #[test]
    fn error_spans_produce_error_severity_logs() {
        let spans = generate_spans(7, NOW);
        let logs = generate_logs(7, NOW);
        let logs_by_span: HashMap<&str, &GeneratedLog> =
            logs.iter().map(|l| (l.span_id.as_str(), l)).collect();

        for span in &spans {
            let log = logs_by_span.get(span.span_id.as_str()).unwrap();
            if span.status_code == "ERROR" {
                assert_eq!(log.severity_number, 17);
                assert_eq!(log.severity_text, "ERROR");
            } else {
                assert_ne!(log.severity_number, 17);
            }
        }
    }

    #[test]
    fn logs_carry_trace_and_span_correlation() {
        let spans = generate_spans(7, NOW);
        let logs = generate_logs(7, NOW);
        let logs_by_span: HashMap<&str, &GeneratedLog> =
            logs.iter().map(|l| (l.span_id.as_str(), l)).collect();

        for span in &spans {
            let log = logs_by_span.get(span.span_id.as_str()).unwrap();
            assert_eq!(log.trace_id, span.trace_id);
            assert_eq!(log.service_name, span.service_name);
            assert_eq!(log.timestamp_unix_nano, span.start_time_unix_nano);
        }
    }

    // ── metrics ───────────────────────────────────────────────────────────

    #[test]
    fn metrics_same_seed_produces_identical_output() {
        let (series_a, points_a) = generate_metrics(42, NOW);
        let (series_b, points_b) = generate_metrics(42, NOW);
        assert_eq!(series_a, series_b);
        assert_eq!(points_a, points_b);
    }

    #[test]
    fn one_series_per_service_per_metric_def() {
        let (series, _) = generate_metrics(7, NOW);
        assert_eq!(series.len(), TOPOLOGY.len() * METRIC_DEFS.len());
    }

    #[test]
    fn each_series_has_the_configured_point_count() {
        let (series, points) = generate_metrics(7, NOW);
        for s in &series {
            let count = points
                .iter()
                .filter(|p| p.metric_series_id == s.metric_series_id)
                .count();
            assert_eq!(count, METRIC_POINT_COUNT as usize);
        }
    }

    #[test]
    fn metric_points_are_within_the_configured_window_and_ordered() {
        let (series, points) = generate_metrics(7, NOW);
        for s in &series {
            let mut last: Option<i64> = None;
            for p in points
                .iter()
                .filter(|p| p.metric_series_id == s.metric_series_id)
            {
                let t: i64 = p.time_unix_nano.parse().unwrap();
                assert!(t <= NOW);
                assert!(NOW - t <= WINDOW_SECS as i64 * 1_000_000_000);
                if let Some(prev) = last {
                    assert!(t >= prev, "points must be non-decreasing in time");
                }
                last = Some(t);
            }
        }
    }
}
