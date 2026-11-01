use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardinalityExceeded {
    pub tenant_id: Uuid,
    pub budget: u64,
    pub current_count: u64,
    pub requested_series: usize,
}

impl std::fmt::Display for CardinalityExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "metric cardinality budget exceeded for tenant {} (budget: {}, current: {}, requested: {})",
            self.tenant_id, self.budget, self.current_count, self.requested_series
        )
    }
}

impl std::error::Error for CardinalityExceeded {}

/// Tracks cumulative metric series submissions per tenant against a configurable budget.
/// Supports both observation mode (warnings only) and enforcement mode (rejection with 429).
pub struct MetricCardinalityBudget {
    counts: Mutex<HashMap<Uuid, u64>>,
    tenant_budgets: Mutex<HashMap<Uuid, u64>>,
    default_budget: u64,
    enforce: bool,
}

impl MetricCardinalityBudget {
    pub fn new(budget: u64) -> Arc<Self> {
        Self::with_enforce(budget, false)
    }

    pub fn with_enforce(budget: u64, enforce: bool) -> Arc<Self> {
        Arc::new(Self {
            counts: Mutex::new(HashMap::new()),
            tenant_budgets: Mutex::new(HashMap::new()),
            default_budget: budget,
            enforce,
        })
    }

    /// Set an explicit metric series budget for a specific tenant.
    pub fn set_tenant_budget(&self, tenant_id: Uuid, budget: u64) {
        let mut map = self
            .tenant_budgets
            .lock()
            .expect("cardinality lock poisoned");
        map.insert(tenant_id, budget);
    }

    /// Retrieve the effective budget for a tenant (custom or default).
    pub fn budget_for(&self, tenant_id: Uuid) -> u64 {
        self.tenant_budgets
            .lock()
            .expect("cardinality lock poisoned")
            .get(&tenant_id)
            .copied()
            .unwrap_or(self.default_budget)
    }

    pub fn is_enforcing(&self) -> bool {
        self.enforce
    }

    /// Check series submission against budget and increment if permitted.
    /// In non-enforcing mode, always increments and returns Ok.
    /// In enforcing mode, returns Err(CardinalityExceeded) without incrementing if budget would be exceeded.
    pub fn check_and_observe(
        &self,
        tenant_id: Uuid,
        series_count: usize,
    ) -> Result<u64, CardinalityExceeded> {
        if series_count == 0 {
            return Ok(self.current_count(tenant_id));
        }

        let effective_budget = self.budget_for(tenant_id);
        let mut map = self.counts.lock().expect("cardinality lock poisoned");
        let total = map.entry(tenant_id).or_insert(0);

        if self.enforce && total.saturating_add(series_count as u64) > effective_budget {
            tracing::warn!(
                tenant_id = %tenant_id,
                current_series = *total,
                requested_series = series_count,
                budget = effective_budget,
                "metric cardinality budget exceeded for tenant — rejecting ingest"
            );
            return Err(CardinalityExceeded {
                tenant_id,
                budget: effective_budget,
                current_count: *total,
                requested_series: series_count,
            });
        }

        *total = total.saturating_add(series_count as u64);
        let new_total = *total;

        if new_total >= effective_budget {
            tracing::warn!(
                tenant_id = %tenant_id,
                series_count = new_total,
                budget = effective_budget,
                "metric cardinality budget reached for tenant"
            );
        }

        Ok(new_total)
    }

    /// Increment the series count for `tenant_id` by `series_count`.
    /// Emits a warning when the cumulative total meets or exceeds the budget.
    pub fn observe(&self, tenant_id: Uuid, series_count: usize) {
        let _ = self.check_and_observe(tenant_id, series_count);
    }

    pub fn current_count(&self, tenant_id: Uuid) -> u64 {
        self.counts
            .lock()
            .expect("cardinality lock poisoned")
            .get(&tenant_id)
            .copied()
            .unwrap_or(0)
    }

    pub fn reset(&self, tenant_id: Uuid) {
        let mut map = self.counts.lock().expect("cardinality lock poisoned");
        map.remove(&tenant_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_accumulates_series_count() {
        let budget = MetricCardinalityBudget::new(1000);
        let tid = Uuid::new_v4();
        budget.observe(tid, 100);
        assert_eq!(budget.current_count(tid), 100);
        budget.observe(tid, 50);
        assert_eq!(budget.current_count(tid), 150);
    }

    #[test]
    fn observe_tracks_tenants_independently() {
        let budget = MetricCardinalityBudget::new(1000);
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        budget.observe(t1, 200);
        budget.observe(t2, 300);
        assert_eq!(budget.current_count(t1), 200);
        assert_eq!(budget.current_count(t2), 300);
    }

    #[test]
    fn observe_detects_budget_exhaustion() {
        let budget = MetricCardinalityBudget::new(100);
        let tid = Uuid::new_v4();
        budget.observe(tid, 50);
        assert_eq!(budget.current_count(tid), 50);
        // Pushing over budget: 50 + 60 = 110 >= 100
        budget.observe(tid, 60);
        assert_eq!(budget.current_count(tid), 110);
    }

    #[test]
    fn observe_at_exact_budget_boundary() {
        let budget = MetricCardinalityBudget::new(100);
        let tid = Uuid::new_v4();
        budget.observe(tid, 100);
        assert_eq!(budget.current_count(tid), 100);
    }

    #[test]
    fn enforce_mode_rejects_exceeded_budget() {
        let budget = MetricCardinalityBudget::with_enforce(100, true);
        let tid = Uuid::new_v4();

        // 80 series allowed
        assert!(budget.check_and_observe(tid, 80).is_ok());
        assert_eq!(budget.current_count(tid), 80);

        // 30 more would make 110 > 100 -> rejected
        let res = budget.check_and_observe(tid, 30);
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert_eq!(err.budget, 100);
        assert_eq!(err.current_count, 80);
        assert_eq!(err.requested_series, 30);

        // Current count should remain 80 (batch was rejected without mutating count)
        assert_eq!(budget.current_count(tid), 80);

        // 20 more fits exactly into budget: 80 + 20 = 100
        assert!(budget.check_and_observe(tid, 20).is_ok());
        assert_eq!(budget.current_count(tid), 100);
    }

    #[test]
    fn custom_tenant_budget_overrides_default() {
        let budget = MetricCardinalityBudget::with_enforce(100, true);
        let t_vip = Uuid::new_v4();
        budget.set_tenant_budget(t_vip, 500);

        // t_vip can submit 300 series
        assert!(budget.check_and_observe(t_vip, 300).is_ok());
        assert_eq!(budget.current_count(t_vip), 300);

        // Normal tenant still has 100 budget
        let t_normal = Uuid::new_v4();
        assert!(budget.check_and_observe(t_normal, 101).is_err());
    }

    #[test]
    fn reset_clears_tenant_count() {
        let budget = MetricCardinalityBudget::new(100);
        let tid = Uuid::new_v4();
        budget.observe(tid, 90);
        assert_eq!(budget.current_count(tid), 90);
        budget.reset(tid);
        assert_eq!(budget.current_count(tid), 0);
    }
}
