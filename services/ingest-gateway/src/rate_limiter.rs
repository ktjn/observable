use governor::{DefaultDirectRateLimiter, DefaultKeyedRateLimiter, Quota, RateLimiter};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// Tenant-aware rate limiter supporting global default quotas and per-tenant custom limits.
pub struct TenantRateLimiter {
    default_limiter: Arc<DefaultKeyedRateLimiter<Uuid>>,
    custom_limiters: Mutex<HashMap<Uuid, Arc<DefaultDirectRateLimiter>>>,
    default_per_second: u32,
}

impl TenantRateLimiter {
    pub fn new(per_second: u32) -> Arc<Self> {
        let quota = Quota::per_second(
            NonZeroU32::new(per_second).expect("rate limit per second must be > 0"),
        );
        Arc::new(Self {
            default_limiter: Arc::new(RateLimiter::keyed(quota)),
            custom_limiters: Mutex::new(HashMap::new()),
            default_per_second: per_second,
        })
    }

    /// Set an explicit per-second quota for a specific tenant.
    pub fn set_tenant_limit(&self, tenant_id: Uuid, per_second: u32) {
        let quota = Quota::per_second(
            NonZeroU32::new(per_second).expect("rate limit per second must be > 0"),
        );
        let limiter = Arc::new(RateLimiter::direct(quota));
        let mut map = self
            .custom_limiters
            .lock()
            .expect("rate limiter lock poisoned");
        map.insert(tenant_id, limiter);
    }

    /// Check whether a request is allowed for `tenant_id`.
    /// Returns Ok(()) if within quota, Err(()) if rate limit exceeded.
    pub fn check_key(&self, tenant_id: &Uuid) -> Result<(), ()> {
        let custom = {
            let map = self
                .custom_limiters
                .lock()
                .expect("rate limiter lock poisoned");
            map.get(tenant_id).cloned()
        };
        if let Some(limiter) = custom {
            limiter.check().map_err(|_| ())
        } else {
            self.default_limiter.check_key(tenant_id).map_err(|_| ())
        }
    }

    pub fn default_limit(&self) -> u32 {
        self.default_per_second
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_limit_applies_per_tenant() {
        let limiter = TenantRateLimiter::new(2);
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();

        assert!(limiter.check_key(&t1).is_ok());
        assert!(limiter.check_key(&t1).is_ok());
        // Third request from t1 within 1 sec should be rate limited
        assert!(limiter.check_key(&t1).is_err());

        // t2 should still have quota (tenant isolation)
        assert!(limiter.check_key(&t2).is_ok());
    }

    #[test]
    fn custom_tenant_limit_overrides_default() {
        let limiter = TenantRateLimiter::new(10);
        let t_custom = Uuid::new_v4();
        // Give t_custom a strict 1 req/sec limit
        limiter.set_tenant_limit(t_custom, 1);

        assert!(limiter.check_key(&t_custom).is_ok());
        assert!(limiter.check_key(&t_custom).is_err());

        // Normal tenant still has default 10 req/sec limit
        let t_normal = Uuid::new_v4();
        assert!(limiter.check_key(&t_normal).is_ok());
        assert!(limiter.check_key(&t_normal).is_ok());
    }
}
