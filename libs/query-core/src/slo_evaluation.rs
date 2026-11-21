//! Pure SLO burn-rate math, moved verbatim from
//! `services/alert-evaluator/src/evaluator.rs` so both the real
//! `alert-evaluator` service and the playground's local WASM engine
//! (`libs/playground-wasm`) evaluate the same logic — no sqlx/tokio/
//! clickhouse dependencies, compiles natively and to
//! `wasm32-unknown-unknown`. See ADR-047 §3.

use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, PartialEq)]
pub enum EvalResult {
    Firing,
    Ok,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SloBurnRateCondition {
    pub slo_id: Uuid,
    pub fast_window_minutes: u64,
    pub slow_window_minutes: u64,
}

pub fn calculate_burn_rate(
    bad_events: u64,
    total_events: u64,
    target: f64,
    _window_minutes: u64,
    _slo_window_days: i32,
) -> f64 {
    if bad_events == 0 || total_events == 0 {
        return 0.0;
    }
    let error_budget_fraction = 1.0 - target;
    let observed_error_fraction = bad_events as f64 / total_events as f64;
    observed_error_fraction / error_budget_fraction
}

pub fn evaluate_burn_rate(
    fast_burn_rate: f64,
    slow_burn_rate: f64,
    fast_threshold: f64,
    slow_threshold: f64,
    _condition: &SloBurnRateCondition,
) -> EvalResult {
    if fast_burn_rate >= fast_threshold && slow_burn_rate >= slow_threshold {
        EvalResult::Firing
    } else {
        EvalResult::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burn_rate_uses_error_rate_over_error_budget() {
        let burn = calculate_burn_rate(10, 1_000, 0.999, 60, 30);
        assert!((burn - 10.0).abs() < 0.001);
    }

    #[test]
    fn multi_window_requires_fast_and_slow_to_fire() {
        let cond = SloBurnRateCondition {
            slo_id: Uuid::nil(),
            fast_window_minutes: 60,
            slow_window_minutes: 360,
        };
        assert_eq!(
            evaluate_burn_rate(15.0, 2.0, 14.4, 1.0, &cond),
            EvalResult::Firing
        );
        assert_eq!(
            evaluate_burn_rate(15.0, 0.5, 14.4, 1.0, &cond),
            EvalResult::Ok
        );
    }
}
