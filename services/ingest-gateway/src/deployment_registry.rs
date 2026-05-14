use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use uuid::Uuid;

const CACHE_TTL: Duration = Duration::from_secs(30);

#[derive(Clone, Hash, Eq, PartialEq, Debug)]
struct CacheKey {
    tenant_id: Uuid,
    service_name: String,
    environment: String,
    service_version: String,
}

struct CacheEntry {
    deployment_id: String,
    fetched_at: Instant,
}

/// Thread-safe registry that resolves the active deployment marker for a
/// (tenant, service, environment, version) tuple.
///
/// Results are cached for 30 s to bound PostgreSQL load at high ingest rates.
/// On DB error the lookup returns an empty string so that ingestion is never
/// blocked by a failing deployment-marker query.
pub struct DeploymentRegistry {
    db: Arc<PgPool>,
    cache: RwLock<HashMap<CacheKey, CacheEntry>>,
}

impl DeploymentRegistry {
    pub fn new(db: Arc<PgPool>) -> Arc<Self> {
        Arc::new(Self {
            db,
            cache: RwLock::new(HashMap::new()),
        })
    }

    /// Return the deployment_id for the most-recent active or in-progress
    /// deployment matching the given coordinates.
    ///
    /// When `service_version` is empty the query matches any version, returning
    /// the latest deployment for the service in that environment.
    pub async fn lookup(
        &self,
        tenant_id: Uuid,
        service_name: &str,
        environment: &str,
        service_version: &str,
    ) -> String {
        let key = CacheKey {
            tenant_id,
            service_name: service_name.to_string(),
            environment: environment.to_string(),
            service_version: service_version.to_string(),
        };

        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&key)
                && entry.fetched_at.elapsed() < CACHE_TTL
            {
                return entry.deployment_id.clone();
            }
        }

        let deployment_id = self.fetch_from_db(&key).await;
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                key,
                CacheEntry {
                    deployment_id: deployment_id.clone(),
                    fetched_at: Instant::now(),
                },
            );
        }
        deployment_id
    }

    async fn fetch_from_db(&self, key: &CacheKey) -> String {
        let result: Option<(Uuid,)> = sqlx::query_as(
            "SELECT deployment_id FROM deployment_markers \
             WHERE tenant_id = $1 \
               AND service_name = $2 \
               AND environment = $3 \
               AND ($4 = '' OR service_version = $4) \
               AND status IN ('in_progress', 'success') \
             ORDER BY started_at DESC \
             LIMIT 1",
        )
        .bind(key.tenant_id)
        .bind(&key.service_name)
        .bind(&key.environment)
        .bind(&key.service_version)
        .fetch_optional(self.db.as_ref())
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                error = %e,
                service_name = %key.service_name,
                "deployment registry DB lookup failed; stamping empty deployment_id"
            );
            None
        });

        result.map(|(id,)| id.to_string()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disconnected_registry() -> Arc<DeploymentRegistry> {
        let pool = Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap());
        DeploymentRegistry::new(pool)
    }

    #[tokio::test]
    async fn lookup_returns_empty_on_db_error() {
        let registry = disconnected_registry();
        let result = registry
            .lookup(Uuid::new_v4(), "svc", "prod", "v1.0.0")
            .await;
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn cache_is_populated_after_first_lookup() {
        let registry = disconnected_registry();
        let tid = Uuid::new_v4();
        registry.lookup(tid, "svc", "staging", "v1").await;
        let cache = registry.cache.read().await;
        let key = CacheKey {
            tenant_id: tid,
            service_name: "svc".into(),
            environment: "staging".into(),
            service_version: "v1".into(),
        };
        assert!(
            cache.contains_key(&key),
            "cache must hold entry after lookup"
        );
    }

    #[tokio::test]
    async fn stale_cache_entry_is_replaced() {
        let registry = disconnected_registry();
        let tid = Uuid::new_v4();
        let key = CacheKey {
            tenant_id: tid,
            service_name: "svc".into(),
            environment: "prod".into(),
            service_version: "v2".into(),
        };
        {
            let mut cache = registry.cache.write().await;
            cache.insert(
                key.clone(),
                CacheEntry {
                    deployment_id: "old-id".into(),
                    fetched_at: Instant::now() - CACHE_TTL - Duration::from_secs(1),
                },
            );
        }
        // Lookup bypasses stale entry and re-fetches (returns "" from disconnected DB).
        let result = registry.lookup(tid, "svc", "prod", "v2").await;
        assert_eq!(result, "");
        let cache = registry.cache.read().await;
        let entry = cache.get(&key).unwrap();
        assert_ne!(
            entry.deployment_id, "old-id",
            "stale entry must be replaced"
        );
    }
}
