use domain::DeploymentMarkerEvent;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Deployment statuses that should still be stamped onto ingested telemetry.
/// Mirrors the `status IN ('in_progress', 'success')` filter the old
/// Postgres-backed lookup used.
const ACTIVE_STATUSES: &[&str] = &["in_progress", "success"];

#[derive(Clone, Debug)]
struct DeploymentState {
    deployment_id: Uuid,
    tenant_id: Uuid,
    service_name: String,
    environment: String,
    service_version: String,
    status: String,
    started_at_unix_nano: u64,
}

/// In-process registry that resolves the active deployment marker for a
/// (tenant, service, environment, version) tuple, fed entirely by
/// `deployment.markers.v1` Kafka events published by admin-service (Phase 5
/// "clean ingest", docs/component-decomposition.md) -- no direct Postgres
/// dependency. Each event carries a deployment's *current* status (not a
/// diff), keyed by `deployment_id`, so applying one just replaces that
/// deployment's prior state; `lookup` scans the (small -- deploy events are
/// orders of magnitude rarer than spans) in-memory set for the most recent
/// match each call, exactly mirroring the old SQL query's semantics
/// (`ORDER BY started_at DESC LIMIT 1`, status filter, optional version
/// wildcard) rather than maintaining secondary indexes.
pub struct DeploymentRegistry {
    deployments: RwLock<HashMap<Uuid, DeploymentState>>,
}

impl DeploymentRegistry {
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            deployments: RwLock::new(HashMap::new()),
        })
    }

    /// Apply an event from `deployment.markers.v1`, replacing any prior state
    /// for that `deployment_id`.
    pub async fn apply_event(&self, event: DeploymentMarkerEvent) {
        let mut deployments = self.deployments.write().await;
        deployments.insert(
            event.deployment_id,
            DeploymentState {
                deployment_id: event.deployment_id,
                tenant_id: event.tenant_id,
                service_name: event.service_name,
                environment: event.environment,
                service_version: event.service_version,
                status: event.status,
                started_at_unix_nano: event.started_at_unix_nano,
            },
        );
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
        let deployments = self.deployments.read().await;
        deployments
            .values()
            .filter(|d| {
                d.tenant_id == tenant_id
                    && d.service_name == service_name
                    && d.environment == environment
                    && (service_version.is_empty() || d.service_version == service_version)
                    && ACTIVE_STATUSES.contains(&d.status.as_str())
            })
            .max_by_key(|d| d.started_at_unix_nano)
            .map(|d| d.deployment_id.to_string())
            .unwrap_or_default()
    }
}

/// Consumes `deployment.markers.v1` and applies each event to `registry`.
/// Runs indefinitely; intended to be spawned as a background task for the
/// process lifetime. Uses auto-commit (unlike storage-writer's durable
/// `NormalizedConsumer`) since a missed or re-delivered event only affects
/// cache freshness, not correctness -- there is no downstream write to
/// guard.
pub async fn run_consumer(
    brokers: &str,
    group_id: &str,
    topic: &str,
    registry: std::sync::Arc<DeploymentRegistry>,
) -> anyhow::Result<()> {
    use rdkafka::{
        ClientConfig, Message,
        consumer::{Consumer, StreamConsumer},
    };

    let consumer: StreamConsumer = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .set("group.id", group_id)
        .set("auto.offset.reset", "earliest")
        .set("enable.auto.commit", "true")
        .create()?;
    consumer.subscribe(&[topic])?;

    loop {
        let msg = consumer.recv().await?;
        let Some(payload) = msg.payload() else {
            continue;
        };
        match serde_json::from_slice::<DeploymentMarkerEvent>(payload) {
            Ok(event) => registry.apply_event(event).await,
            Err(e) => tracing::warn!(error = %e, "deployment marker event deserialise failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(
        deployment_id: Uuid,
        tenant_id: Uuid,
        service_name: &str,
        environment: &str,
        service_version: &str,
        status: &str,
        started_at_unix_nano: u64,
    ) -> DeploymentMarkerEvent {
        DeploymentMarkerEvent {
            deployment_id,
            tenant_id,
            service_name: service_name.into(),
            environment: environment.into(),
            service_version: service_version.into(),
            status: status.into(),
            started_at_unix_nano,
        }
    }

    #[tokio::test]
    async fn lookup_resolves_active_deployment_by_service_and_version() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();
        let deployment_id = Uuid::new_v4();

        registry
            .apply_event(event(
                deployment_id,
                tenant_id,
                "api",
                "prod",
                "v2.0.0",
                "in_progress",
                1,
            ))
            .await;

        let result = registry.lookup(tenant_id, "api", "prod", "v2.0.0").await;
        assert_eq!(result, deployment_id.to_string());
    }

    #[tokio::test]
    async fn lookup_matches_success_status() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();
        let deployment_id = Uuid::new_v4();

        registry
            .apply_event(event(
                deployment_id,
                tenant_id,
                "worker",
                "staging",
                "v1.5.0",
                "success",
                1,
            ))
            .await;

        let result = registry
            .lookup(tenant_id, "worker", "staging", "v1.5.0")
            .await;
        assert_eq!(result, deployment_id.to_string());
    }

    #[tokio::test]
    async fn lookup_ignores_failed_and_rolled_back_deployments() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();

        for (i, status) in ["failed", "rolled_back"].iter().enumerate() {
            registry
                .apply_event(event(
                    Uuid::new_v4(),
                    tenant_id,
                    "svc",
                    "prod",
                    "v3.0.0",
                    status,
                    i as u64 + 1,
                ))
                .await;
        }

        let result = registry.lookup(tenant_id, "svc", "prod", "v3.0.0").await;
        assert_eq!(
            result, "",
            "failed/rolled_back deployments must not be stamped"
        );
    }

    #[tokio::test]
    async fn lookup_empty_version_matches_latest_active() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();
        let deployment_id = Uuid::new_v4();

        registry
            .apply_event(event(
                deployment_id,
                tenant_id,
                "frontend",
                "staging",
                "v4.1.0",
                "in_progress",
                1,
            ))
            .await;

        let result = registry.lookup(tenant_id, "frontend", "staging", "").await;
        assert_eq!(result, deployment_id.to_string());
    }

    #[tokio::test]
    async fn lookup_is_tenant_scoped() {
        let registry = DeploymentRegistry::new();
        let tenant_a = Uuid::new_v4();
        let tenant_b = Uuid::new_v4();

        registry
            .apply_event(event(
                Uuid::new_v4(),
                tenant_a,
                "svc",
                "prod",
                "v1.0.0",
                "in_progress",
                1,
            ))
            .await;

        let result = registry.lookup(tenant_b, "svc", "prod", "v1.0.0").await;
        assert_eq!(result, "", "lookup must not cross tenant boundaries");
    }

    #[tokio::test]
    async fn lookup_returns_most_recent_when_multiple_match() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();

        registry
            .apply_event(event(
                Uuid::new_v4(),
                tenant_id,
                "api",
                "prod",
                "v1.0.0",
                "success",
                1,
            ))
            .await;

        let newest_id = Uuid::new_v4();
        registry
            .apply_event(event(
                newest_id,
                tenant_id,
                "api",
                "prod",
                "v1.0.0",
                "in_progress",
                2,
            ))
            .await;

        let result = registry.lookup(tenant_id, "api", "prod", "v1.0.0").await;
        assert_eq!(
            result,
            newest_id.to_string(),
            "must return most recent deployment"
        );
    }

    #[tokio::test]
    async fn finish_event_supersedes_in_progress_status() {
        let registry = DeploymentRegistry::new();
        let tenant_id = Uuid::new_v4();
        let deployment_id = Uuid::new_v4();

        registry
            .apply_event(event(
                deployment_id,
                tenant_id,
                "api",
                "prod",
                "v1.0.0",
                "in_progress",
                1,
            ))
            .await;
        registry
            .apply_event(event(
                deployment_id,
                tenant_id,
                "api",
                "prod",
                "v1.0.0",
                "failed",
                1,
            ))
            .await;

        let result = registry.lookup(tenant_id, "api", "prod", "v1.0.0").await;
        assert_eq!(
            result, "",
            "a finish event replaces the deployment's state, not just adds to it"
        );
    }
}
