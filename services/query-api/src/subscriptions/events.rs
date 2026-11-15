//! The provider change events this facade consumes, and how each one maps onto a
//! registered view (PUSH-018).
//!
//! ## Why these structs are mirrored here
//!
//! `deployment.markers.v1` and `change.markers.v1` are shared wire contracts in
//! `libs/domain`, so those are imported. `health.transitions.v1` and
//! `alerting.state.v1` are currently declared *inside* `health-service` and
//! `alert-evaluator` (`src/queue.rs`) rather than in a shared contract crate, and
//! ADR-035 forbids importing another component's implementation. They are
//! therefore mirrored here as deserialization-only structs, pinned by tests
//! against the exact JSON those producers emit. Promoting both to `libs/domain`
//! beside the other two would make the contract shared and catch drift at compile
//! time; that is a follow-up, as it edits provider-owned crates.
//!
//! ## Reaction policy
//!
//! A provider event is only ever a *freshness signal*; it is never forwarded to a
//! browser as-is. Each event either triggers a re-read through the view's owning
//! query path (`Replace`) or, when the event already carries a trivial keyed
//! upsert for exactly the value shape the view publishes, is passed through
//! (`Delta`). The server never computes a diff.

use super::protocol::ChangeOp;
use super::views::ViewQuery;
use domain::{ChangeMarkerEvent, DeploymentMarkerEvent};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

/// Mirrors `health_service::queue::HealthTransitionEvent` (`health.transitions.v1`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HealthTransitionEvent {
    pub tenant_id: Uuid,
    pub service_id: Uuid,
    pub environment_class: String,
    pub state: String,
    pub evidence_status: String,
    pub evaluated_at_unix_nano: u64,
}

/// Mirrors `alert_evaluator::queue::AlertStateChangeEvent` (`alerting.state.v1`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AlertStateChangeEvent {
    pub tenant_id: Uuid,
    pub incident_id: Uuid,
    pub status: String,
    pub severity: String,
    pub event_type: String,
    pub occurred_at_unix_nano: u64,
}

/// A parsed event from one of the four provider topics.
#[derive(Debug, Clone)]
pub enum ProviderEvent {
    Deployment(DeploymentMarkerEvent),
    Change(ChangeMarkerEvent),
    Health(HealthTransitionEvent),
    AlertState(AlertStateChangeEvent),
}

impl ProviderEvent {
    pub fn tenant_id(&self) -> Uuid {
        match self {
            Self::Deployment(e) => e.tenant_id,
            Self::Change(e) => e.tenant_id,
            Self::Health(e) => e.tenant_id,
            Self::AlertState(e) => e.tenant_id,
        }
    }
}

/// Which provider topic a consumer task is reading, and how to parse it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderTopic {
    DeploymentMarkers,
    ChangeMarkers,
    HealthTransitions,
    AlertingState,
}

impl ProviderTopic {
    pub fn parse_payload(self, payload: &[u8]) -> Result<ProviderEvent, serde_json::Error> {
        Ok(match self {
            Self::DeploymentMarkers => ProviderEvent::Deployment(serde_json::from_slice(payload)?),
            Self::ChangeMarkers => ProviderEvent::Change(serde_json::from_slice(payload)?),
            Self::HealthTransitions => ProviderEvent::Health(serde_json::from_slice(payload)?),
            Self::AlertingState => ProviderEvent::AlertState(serde_json::from_slice(payload)?),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeploymentMarkers => "deployment_markers",
            Self::ChangeMarkers => "change_markers",
            Self::HealthTransitions => "health_transitions",
            Self::AlertingState => "alerting_state",
        }
    }
}

/// What a registered view should do about one provider event.
#[derive(Debug, Clone, PartialEq)]
pub enum Reaction {
    /// Not relevant to this view (different tenant, service, or environment).
    Ignore,
    /// Re-read the view through its owning query path and emit `replace`.
    Replace,
    /// Pass the event's own keyed op straight through as `delta`.
    Delta(Vec<ChangeOp>),
}

/// Decides a view's reaction to a provider event.
///
/// `tenant_id` is the connection's *authoritative session* tenant. An event for
/// any other tenant is always ignored, independently of the per-batch session
/// revalidation that also guards delivery (PUSH-007).
pub fn react(view: &ViewQuery, tenant_id: Uuid, event: &ProviderEvent) -> Reaction {
    if event.tenant_id() != tenant_id {
        return Reaction::Ignore;
    }

    match (view, event) {
        // A deployment marker already carries the exact value this view publishes
        // for that service: `latest_deployment` is the newest marker's
        // `service_version` (admin-service `/internal/deployment-enrichment`).
        // That makes it a genuine pass-through upsert, keyed by service name.
        (ViewQuery::ServiceStatus { environment }, ProviderEvent::Deployment(e)) => {
            if environment
                .as_deref()
                .is_some_and(|env| env != e.environment)
            {
                return Reaction::Ignore;
            }
            Reaction::Delta(vec![ChangeOp::Upsert {
                key: e.service_name.clone(),
                value: json!({ "latest_deployment": e.service_version }),
            }])
        }
        // A change marker is not part of this view's value shape, and an alert
        // state change moves an aggregate count that cannot be derived from one
        // incident's transition. Both re-read authoritatively.
        (ViewQuery::ServiceStatus { environment }, ProviderEvent::Change(e)) => {
            if environment
                .as_deref()
                .is_some_and(|env| env != e.environment)
            {
                return Reaction::Ignore;
            }
            Reaction::Replace
        }
        (ViewQuery::ServiceStatus { .. }, ProviderEvent::AlertState(_)) => Reaction::Replace,

        // Health assessments are gated per resource by health-service, so the
        // authorized read is re-run rather than the event's state passed through.
        (
            ViewQuery::ServiceHealth {
                service_id,
                environment_class,
            },
            ProviderEvent::Health(e),
        ) => {
            if e.service_id == *service_id && e.environment_class == environment_class.as_str() {
                Reaction::Replace
            } else {
                Reaction::Ignore
            }
        }

        _ => Reaction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::views::EnvironmentClass;
    use serde_json::Value;

    const TENANT: u128 = 11;

    fn tenant() -> Uuid {
        Uuid::from_u128(TENANT)
    }

    fn deployment(tenant_id: Uuid, service: &str, env: &str, version: &str) -> ProviderEvent {
        ProviderEvent::Deployment(DeploymentMarkerEvent {
            deployment_id: Uuid::new_v4(),
            tenant_id,
            service_name: service.into(),
            environment: env.into(),
            service_version: version.into(),
            status: "success".into(),
            started_at_unix_nano: 1_700_000_000_000_000_000,
        })
    }

    fn change(tenant_id: Uuid, env: &str) -> ProviderEvent {
        ProviderEvent::Change(ChangeMarkerEvent {
            change_event_id: Uuid::new_v4(),
            tenant_id,
            event_type: "feature_flag".into(),
            service_name: Some("checkout".into()),
            environment: env.into(),
            title: "Enabled new-checkout-flow".into(),
            occurred_at_unix_nano: 1_700_000_000_000_000_000,
        })
    }

    fn health(tenant_id: Uuid, service_id: Uuid, class: &str) -> ProviderEvent {
        ProviderEvent::Health(HealthTransitionEvent {
            tenant_id,
            service_id,
            environment_class: class.into(),
            state: "degraded".into(),
            evidence_status: "current".into(),
            evaluated_at_unix_nano: 1_700_000_000_000_000_000,
        })
    }

    fn alert(tenant_id: Uuid) -> ProviderEvent {
        ProviderEvent::AlertState(AlertStateChangeEvent {
            tenant_id,
            incident_id: Uuid::new_v4(),
            status: "resolved".into(),
            severity: "critical".into(),
            event_type: "alert_resolved".into(),
            occurred_at_unix_nano: 1_700_000_000_000_000_000,
        })
    }

    fn status_view(environment: Option<&str>) -> ViewQuery {
        ViewQuery::ServiceStatus {
            environment: environment.map(str::to_string),
        }
    }

    fn health_view(service_id: Uuid) -> ViewQuery {
        ViewQuery::ServiceHealth {
            service_id,
            environment_class: EnvironmentClass::Production,
        }
    }

    #[test]
    fn an_event_for_another_tenant_is_always_ignored() {
        let other = Uuid::from_u128(999);
        assert_eq!(
            react(
                &status_view(None),
                tenant(),
                &deployment(other, "a", "prod", "v1")
            ),
            Reaction::Ignore
        );
        assert_eq!(
            react(&status_view(None), tenant(), &alert(other)),
            Reaction::Ignore
        );
        let id = Uuid::new_v4();
        assert_eq!(
            react(&health_view(id), tenant(), &health(other, id, "production")),
            Reaction::Ignore
        );
    }

    #[test]
    fn a_deployment_marker_passes_through_as_a_keyed_upsert() {
        let reaction = react(
            &status_view(None),
            tenant(),
            &deployment(tenant(), "checkout", "production", "v1.2.3"),
        );
        assert_eq!(
            reaction,
            Reaction::Delta(vec![ChangeOp::Upsert {
                key: "checkout".into(),
                value: json!({ "latest_deployment": "v1.2.3" }),
            }])
        );
    }

    #[test]
    fn a_deployment_in_another_environment_is_ignored_by_a_filtered_view() {
        assert_eq!(
            react(
                &status_view(Some("production")),
                tenant(),
                &deployment(tenant(), "checkout", "staging", "v9")
            ),
            Reaction::Ignore
        );
    }

    #[test]
    fn an_alert_state_change_replaces_because_a_count_is_not_derivable() {
        assert_eq!(
            react(&status_view(None), tenant(), &alert(tenant())),
            Reaction::Replace
        );
    }

    #[test]
    fn a_change_marker_replaces_and_respects_the_environment_filter() {
        assert_eq!(
            react(
                &status_view(None),
                tenant(),
                &change(tenant(), "production")
            ),
            Reaction::Replace
        );
        assert_eq!(
            react(
                &status_view(Some("production")),
                tenant(),
                &change(tenant(), "staging")
            ),
            Reaction::Ignore
        );
    }

    #[test]
    fn a_health_transition_replaces_only_its_own_service_and_class() {
        let id = Uuid::new_v4();
        assert_eq!(
            react(
                &health_view(id),
                tenant(),
                &health(tenant(), id, "production")
            ),
            Reaction::Replace
        );
        assert_eq!(
            react(
                &health_view(id),
                tenant(),
                &health(tenant(), id, "non_production")
            ),
            Reaction::Ignore
        );
        assert_eq!(
            react(
                &health_view(id),
                tenant(),
                &health(tenant(), Uuid::new_v4(), "production")
            ),
            Reaction::Ignore
        );
    }

    #[test]
    fn views_ignore_events_belonging_to_the_other_view_kind() {
        let id = Uuid::new_v4();
        assert_eq!(
            react(&health_view(id), tenant(), &alert(tenant())),
            Reaction::Ignore
        );
        assert_eq!(
            react(
                &status_view(None),
                tenant(),
                &health(tenant(), id, "production")
            ),
            Reaction::Ignore
        );
    }

    /// Pins the mirrored structs against the exact field names the provider
    /// crates' own serialization tests assert they emit.
    #[test]
    fn the_mirrored_health_contract_parses_the_producers_payload() {
        let tenant_id = Uuid::new_v4();
        let service_id = Uuid::new_v4();
        let payload = json!({
            "tenant_id": tenant_id,
            "service_id": service_id,
            "environment_class": "production",
            "state": "degraded",
            "evidence_status": "current",
            "evaluated_at_unix_nano": 1_700_000_000_000_000_000_u64,
        });
        let event = ProviderTopic::HealthTransitions
            .parse_payload(&serde_json::to_vec(&payload).unwrap())
            .expect("health.transitions.v1 payload parses");
        match event {
            ProviderEvent::Health(e) => {
                assert_eq!(e.tenant_id, tenant_id);
                assert_eq!(e.service_id, service_id);
                assert_eq!(e.state, "degraded");
                assert_eq!(e.evidence_status, "current");
            }
            other => panic!("expected a health event, got {other:?}"),
        }
    }

    #[test]
    fn the_mirrored_alerting_contract_parses_the_producers_payload() {
        let tenant_id = Uuid::new_v4();
        let incident_id = Uuid::new_v4();
        let payload = json!({
            "tenant_id": tenant_id,
            "incident_id": incident_id,
            "status": "resolved",
            "severity": "critical",
            "event_type": "alert_resolved",
            "occurred_at_unix_nano": 1_700_000_000_000_000_000_u64,
        });
        let event = ProviderTopic::AlertingState
            .parse_payload(&serde_json::to_vec(&payload).unwrap())
            .expect("alerting.state.v1 payload parses");
        match event {
            ProviderEvent::AlertState(e) => {
                assert_eq!(e.incident_id, incident_id);
                assert_eq!(e.status, "resolved");
                assert_eq!(e.event_type, "alert_resolved");
            }
            other => panic!("expected an alert event, got {other:?}"),
        }
    }

    #[test]
    fn a_payload_from_the_wrong_topic_is_a_parse_error_not_a_silent_event() {
        let health_payload = json!({
            "tenant_id": Uuid::new_v4(),
            "service_id": Uuid::new_v4(),
            "environment_class": "production",
            "state": "normal",
            "evidence_status": "current",
            "evaluated_at_unix_nano": 1_u64,
        });
        let bytes = serde_json::to_vec(&health_payload).unwrap();
        assert!(ProviderTopic::AlertingState.parse_payload(&bytes).is_err());
    }

    #[test]
    fn a_non_json_payload_is_a_parse_error() {
        assert!(
            ProviderTopic::DeploymentMarkers
                .parse_payload(b"not json")
                .is_err()
        );
    }

    #[test]
    fn deployment_and_change_contracts_come_from_the_shared_domain_crate() {
        // Guards against the mirrored-struct drift risk documented above: these two
        // are imported, so a contract change is a compile error, not a parse bug.
        let payload: Value = json!({
            "deployment_id": Uuid::new_v4(),
            "tenant_id": Uuid::new_v4(),
            "service_name": "checkout",
            "environment": "production",
            "service_version": "v1",
            "status": "success",
            "started_at_unix_nano": 1_u64,
        });
        assert!(
            ProviderTopic::DeploymentMarkers
                .parse_payload(&serde_json::to_vec(&payload).unwrap())
                .is_ok()
        );
    }
}
