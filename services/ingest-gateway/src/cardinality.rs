use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Tracks cumulative metric series submissions per tenant against a configurable budget.
/// Observation only — ingest is never rejected based on this counter.
pub struct MetricCardinalityBudget {
    counts: Mutex<HashMap<Uuid, u64>>,
    budget: u64,
}

impl MetricCardinalityBudget {
    pub fn new(budget: u64) -> Arc<Self> {
        Arc::new(Self {
            counts: Mutex::new(HashMap::new()),
            budget,
        })
    }

    /// Increment the series count for `tenant_id` by `series_count`.
    /// Emits a warning when the cumulative total meets or exceeds the budget.
    pub fn observe(&self, tenant_id: Uuid, series_count: usize) {
        let current = {
            let mut map = self.counts.lock().expect("cardinality lock poisoned");
            let total = map.entry(tenant_id).or_insert(0);
            *total = total.saturating_add(series_count as u64);
            *total
        };
        if current >= self.budget {
            tracing::warn!(
                tenant_id = %tenant_id,
                series_count = current,
                budget = self.budget,
                "metric cardinality budget exceeded for tenant"
            );
        }
    }

    pub fn current_count(&self, tenant_id: Uuid) -> u64 {
        self.counts
            .lock()
            .expect("cardinality lock poisoned")
            .get(&tenant_id)
            .copied()
            .unwrap_or(0)
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
}
