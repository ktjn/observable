//! `decide-access` contract (ADR-041): wire types, the pure policy evaluation shared by the
//! real provider (`observable-auth`, PostgreSQL policy store) and its programmable mock, an
//! HTTP client for consumers, and the provider-owned mock.
//!
//! Semantics:
//! - Fail closed: a missing tenant or principal, an unknown principal, an unknown resource
//!   type, a missing classification or an unavailable policy store never yields `permit`.
//! - Existence hiding: every deny has the same wire response (`403 {"decision":"deny"}`),
//!   whatever the reason; the reason is only written to the provider's decision log.
//! - Purpose: a request that policy marks purpose-bound and states no purpose is answered with
//!   `422 purpose_missing` (decide) or excluded (filter), and is recorded.
//! - Elevation: a stated elevation grant that is expired, revoked or foreign is a deny; a valid
//!   grant raises the permitted classification for its resource type.
//! - Filter returns only the permitted subset, never a count of denied resources.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ordered data classification of a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Public,
    Internal,
    Confidential,
    Restricted,
}

impl Classification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Internal => "internal",
            Self::Confidential => "confidential",
            Self::Restricted => "restricted",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "public" => Some(Self::Public),
            "internal" => Some(Self::Internal),
            "confidential" => Some(Self::Confidential),
            "restricted" => Some(Self::Restricted),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    /// A human user; the provider resolves the role from the tenant membership.
    User,
    /// An API-key caller already verified through `validate-credential`; the role is the
    /// one that validation returned.
    Service,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
    pub kind: PrincipalKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

impl Principal {
    pub fn user(id: Uuid) -> Self {
        Self {
            kind: PrincipalKind::User,
            id: Some(id),
            role: None,
        }
    }

    pub fn service(role: &str) -> Self {
        Self {
            kind: PrincipalKind::Service,
            id: None,
            role: Some(role.to_owned()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    pub resource_type: String,
    pub id: String,
    /// Owner-supplied classification. A resource the owner cannot resolve is sent without
    /// one and is denied exactly like a restricted resource.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classification: Option<Classification>,
}

impl ResourceRef {
    pub fn new(resource_type: &str, id: &str, classification: Classification) -> Self {
        Self {
            resource_type: resource_type.to_owned(),
            id: id.to_owned(),
            classification: Some(classification),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceKey {
    pub resource_type: String,
    pub id: String,
}

/// `POST /internal/authz/decide`. Every field is optional on the wire so that an incomplete
/// request is denied (fail closed) instead of rejected as malformed.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DecideRequest {
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
    #[serde(default)]
    pub principal: Option<Principal>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resource: Option<ResourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_id: Option<Uuid>,
}

/// `POST /internal/authz/filter`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FilterRequest {
    #[serde(default)]
    pub tenant_id: Option<Uuid>,
    #[serde(default)]
    pub principal: Option<Principal>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub resources: Vec<ResourceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevation_id: Option<Uuid>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilterResponse {
    pub permitted: Vec<ResourceKey>,
}

/// Consumer-visible decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Permit,
    Deny,
    PurposeMissing,
}

/// The policy store could not decide; consumers fail closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecisionUnavailable;

/// Internal outcome, including the deny reason recorded in the decision log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Permit,
    PurposeMissing,
    Deny(DenyReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenyReason {
    IncompleteRequest,
    UnknownPrincipal,
    UnknownResource,
    NoGrant,
    ElevationExpired,
    ElevationInvalid,
}

impl Outcome {
    pub fn decision(self) -> Decision {
        match self {
            Self::Permit => Decision::Permit,
            Self::PurposeMissing => Decision::PurposeMissing,
            Self::Deny(_) => Decision::Deny,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Permit => "permit",
            Self::PurposeMissing => "purpose_missing",
            Self::Deny(_) => "deny",
        }
    }

    pub fn reason(self) -> Option<&'static str> {
        match self {
            Self::Deny(r) => Some(match r {
                DenyReason::IncompleteRequest => "incomplete_request",
                DenyReason::UnknownPrincipal => "unknown_principal",
                DenyReason::UnknownResource => "unknown_resource",
                DenyReason::NoGrant => "no_grant",
                DenyReason::ElevationExpired => "elevation_expired",
                DenyReason::ElevationInvalid => "elevation_invalid",
            }),
            Self::PurposeMissing => Some("purpose_missing"),
            Self::Permit => None,
        }
    }
}

/// A role may perform `action` on `resource_type` up to `max_classification`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub role: String,
    pub resource_type: String,
    pub action: String,
    pub max_classification: Classification,
}

/// `action` on `resource_type` at or above `min_classification` requires a stated purpose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurposeRule {
    pub resource_type: String,
    pub action: String,
    pub min_classification: Classification,
}

/// A time-limited elevation grant held by one user in one tenant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Elevation {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub user_id: Uuid,
    pub resource_type: String,
    pub max_classification: Classification,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
}

/// Everything the evaluation needs for one principal in one tenant.
#[derive(Clone, Debug, Default)]
pub struct PolicySnapshot {
    /// Effective role; `None` when the principal has no membership in the tenant.
    pub role: Option<String>,
    pub grants: Vec<Grant>,
    pub purpose_rules: Vec<PurposeRule>,
    /// The elevation named by the request, when it exists.
    pub elevation: Option<Elevation>,
}

/// Validated request context for evaluation.
#[derive(Clone, Debug)]
pub struct Subject<'a> {
    pub tenant_id: Uuid,
    pub principal: &'a Principal,
    pub action: &'a str,
    pub purpose: Option<&'a str>,
    pub elevation_id: Option<Uuid>,
}

/// Pure policy evaluation shared by the provider and the mock.
pub fn evaluate(
    snapshot: &PolicySnapshot,
    subject: &Subject<'_>,
    resource: &ResourceRef,
    now: DateTime<Utc>,
) -> Outcome {
    let Some(role) = snapshot.role.as_deref() else {
        return Outcome::Deny(DenyReason::UnknownPrincipal);
    };
    let Some(classification) = resource.classification else {
        return Outcome::Deny(DenyReason::UnknownResource);
    };
    if !snapshot
        .grants
        .iter()
        .any(|g| g.resource_type == resource.resource_type)
    {
        return Outcome::Deny(DenyReason::UnknownResource);
    }

    let mut ceiling = snapshot
        .grants
        .iter()
        .filter(|g| {
            g.role == role
                && g.resource_type == resource.resource_type
                && g.action == subject.action
        })
        .map(|g| g.max_classification)
        .max();

    if let Some(requested) = subject.elevation_id {
        let valid_owner = |e: &Elevation| {
            e.id == requested
                && e.tenant_id == subject.tenant_id
                && subject.principal.kind == PrincipalKind::User
                && subject.principal.id == Some(e.user_id)
        };
        match snapshot.elevation.as_ref() {
            Some(e) if valid_owner(e) && (e.revoked || e.expires_at <= now) => {
                return Outcome::Deny(DenyReason::ElevationExpired);
            }
            Some(e) if valid_owner(e) => {
                if e.resource_type == resource.resource_type {
                    ceiling = ceiling.max(Some(e.max_classification));
                }
            }
            _ => return Outcome::Deny(DenyReason::ElevationInvalid),
        }
    }

    match ceiling {
        Some(max) if classification <= max => {}
        _ => return Outcome::Deny(DenyReason::NoGrant),
    }

    let purpose_bound = snapshot.purpose_rules.iter().any(|r| {
        r.resource_type == resource.resource_type
            && r.action == subject.action
            && classification >= r.min_classification
    });
    if purpose_bound && subject.purpose.is_none_or(|p| p.trim().is_empty()) {
        return Outcome::PurposeMissing;
    }
    Outcome::Permit
}

/// Consumer-side port for `decide-access`.
#[allow(async_fn_in_trait)]
pub trait AccessDecider {
    async fn decide(&self, request: &DecideRequest) -> Result<Decision, DecisionUnavailable>;
    async fn filter(
        &self,
        request: &FilterRequest,
    ) -> Result<Vec<ResourceKey>, DecisionUnavailable>;
}

/// HTTP adapter for `observable-auth`. Any transport error or unexpected status is
/// `DecisionUnavailable`, so a consumer can only fail closed.
#[derive(Clone)]
pub struct HttpAccessDecider {
    base_url: String,
    internal_token: String,
    client: reqwest::Client,
}

impl HttpAccessDecider {
    pub fn new(
        base_url: impl Into<String>,
        internal_token: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            internal_token: internal_token.into(),
            client,
        }
    }

    async fn post<T: Serialize>(
        &self,
        path: &str,
        body: &T,
    ) -> Result<reqwest::Response, DecisionUnavailable> {
        self.client
            .post(format!("{}{path}", self.base_url))
            .header("X-Internal-Token", &self.internal_token)
            .json(body)
            .send()
            .await
            .map_err(|_| DecisionUnavailable)
    }
}

impl AccessDecider for HttpAccessDecider {
    async fn decide(&self, request: &DecideRequest) -> Result<Decision, DecisionUnavailable> {
        let response = self.post("/internal/authz/decide", request).await?;
        match response.status().as_u16() {
            200 => Ok(Decision::Permit),
            403 => Ok(Decision::Deny),
            422 => Ok(Decision::PurposeMissing),
            _ => Err(DecisionUnavailable),
        }
    }

    async fn filter(
        &self,
        request: &FilterRequest,
    ) -> Result<Vec<ResourceKey>, DecisionUnavailable> {
        let response = self.post("/internal/authz/filter", request).await?;
        if response.status().as_u16() != 200 {
            return Err(DecisionUnavailable);
        }
        let body: FilterResponse = response.json().await.map_err(|_| DecisionUnavailable)?;
        Ok(body.permitted)
    }
}

/// One decision-log entry, as recorded by the provider and the mock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionRecord {
    pub tenant_id: Option<Uuid>,
    pub principal_kind: Option<PrincipalKind>,
    pub principal_id: Option<Uuid>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub action: Option<String>,
    pub purpose: Option<String>,
    pub elevation_id: Option<Uuid>,
    pub decision: String,
    pub reason: Option<String>,
}

/// Programmable failure scenario of the mock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MockScenario {
    /// Evaluate the configured policy.
    #[default]
    Policy,
    /// Policy store unavailable: every call fails closed.
    Unavailable,
    /// Evaluate after a delay (consumer timeout handling).
    Delay(Duration),
}

#[derive(Default)]
struct MockState {
    scenario: MockScenario,
    grants: Vec<Grant>,
    purpose_rules: Vec<PurposeRule>,
    members: HashMap<(Uuid, Uuid), String>,
    elevations: Vec<Elevation>,
    log: Vec<DecisionRecord>,
}

/// Provider-owned programmable mock of `decide-access`. It evaluates the same policy
/// semantics as the provider through [`evaluate`], over in-memory state that a test seeds and
/// resets, and records decisions like the provider's decision log.
#[derive(Clone, Default)]
pub struct MockAccessDecider {
    state: Arc<Mutex<MockState>>,
}

impl MockAccessDecider {
    pub fn new() -> Self {
        Self::default()
    }

    fn with<R>(&self, f: impl FnOnce(&mut MockState) -> R) -> R {
        f(&mut self.state.lock().expect("mock state poisoned"))
    }

    pub fn reset(&self) {
        self.with(|s| *s = MockState::default());
    }

    pub fn set_scenario(&self, scenario: MockScenario) {
        self.with(|s| s.scenario = scenario);
    }

    pub fn grant(&self, role: &str, resource_type: &str, action: &str, max: Classification) {
        self.with(|s| {
            s.grants.push(Grant {
                role: role.into(),
                resource_type: resource_type.into(),
                action: action.into(),
                max_classification: max,
            })
        });
    }

    pub fn require_purpose(&self, resource_type: &str, action: &str, min: Classification) {
        self.with(|s| {
            s.purpose_rules.push(PurposeRule {
                resource_type: resource_type.into(),
                action: action.into(),
                min_classification: min,
            })
        });
    }

    pub fn add_member(&self, tenant_id: Uuid, user_id: Uuid, role: &str) {
        self.with(|s| {
            s.members.insert((tenant_id, user_id), role.into());
        });
    }

    pub fn add_elevation(&self, elevation: Elevation) {
        self.with(|s| s.elevations.push(elevation));
    }

    pub fn decision_log(&self) -> Vec<DecisionRecord> {
        self.with(|s| s.log.clone())
    }

    async fn pause(&self) -> Result<(), DecisionUnavailable> {
        match self.with(|s| s.scenario) {
            MockScenario::Unavailable => Err(DecisionUnavailable),
            MockScenario::Delay(d) => {
                tokio::time::sleep(d).await;
                Ok(())
            }
            MockScenario::Policy => Ok(()),
        }
    }

    fn snapshot(
        s: &MockState,
        tenant_id: Uuid,
        principal: &Principal,
        elevation_id: Option<Uuid>,
    ) -> PolicySnapshot {
        let role = match principal.kind {
            PrincipalKind::User => principal
                .id
                .and_then(|id| s.members.get(&(tenant_id, id)).cloned()),
            PrincipalKind::Service => principal.role.clone(),
        };
        PolicySnapshot {
            role,
            grants: s.grants.clone(),
            purpose_rules: s.purpose_rules.clone(),
            elevation: elevation_id
                .and_then(|id| s.elevations.iter().find(|e| e.id == id).cloned()),
        }
    }
}

/// Validates the common request fields; `None` means the request is incomplete.
pub fn subject_of<'a>(
    tenant_id: Option<Uuid>,
    principal: Option<&'a Principal>,
    action: Option<&'a str>,
    purpose: Option<&'a str>,
    elevation_id: Option<Uuid>,
) -> Option<Subject<'a>> {
    let principal = principal?;
    let complete = match principal.kind {
        PrincipalKind::User => principal.id.is_some(),
        PrincipalKind::Service => principal.role.as_deref().is_some_and(|r| !r.is_empty()),
    };
    let action = action.filter(|a| !a.is_empty())?;
    complete.then_some(Subject {
        tenant_id: tenant_id?,
        principal,
        action,
        purpose,
        elevation_id,
    })
}

/// Builds the decision record for one evaluated (or rejected) resource.
pub fn record_for(
    tenant_id: Option<Uuid>,
    principal: Option<&Principal>,
    action: Option<&str>,
    resource: Option<&ResourceRef>,
    purpose: Option<&str>,
    elevation_id: Option<Uuid>,
    outcome: Outcome,
) -> DecisionRecord {
    DecisionRecord {
        tenant_id,
        principal_kind: principal.map(|p| p.kind),
        principal_id: principal.and_then(|p| p.id),
        resource_type: resource.map(|r| r.resource_type.clone()),
        resource_id: resource.map(|r| r.id.clone()),
        action: action.map(str::to_owned),
        purpose: purpose.map(str::to_owned),
        elevation_id,
        decision: outcome.label().to_owned(),
        reason: outcome.reason().map(str::to_owned),
    }
}

/// Whether a filter evaluation is recorded: purpose- or elevation-bound requests, and
/// purpose-missing exclusions. Decide records every evaluation.
pub fn filter_records(purpose: Option<&str>, elevation_id: Option<Uuid>, outcome: Outcome) -> bool {
    purpose.is_some() || elevation_id.is_some() || outcome == Outcome::PurposeMissing
}

impl AccessDecider for MockAccessDecider {
    async fn decide(&self, r: &DecideRequest) -> Result<Decision, DecisionUnavailable> {
        self.pause().await?;
        let now = Utc::now();
        let outcome = self.with(|s| {
            let outcome = match (
                subject_of(
                    r.tenant_id,
                    r.principal.as_ref(),
                    r.action.as_deref(),
                    r.purpose.as_deref(),
                    r.elevation_id,
                ),
                r.resource.as_ref(),
            ) {
                (Some(subject), Some(resource)) => {
                    let snap = Self::snapshot(
                        s,
                        subject.tenant_id,
                        subject.principal,
                        subject.elevation_id,
                    );
                    evaluate(&snap, &subject, resource, now)
                }
                _ => Outcome::Deny(DenyReason::IncompleteRequest),
            };
            s.log.push(record_for(
                r.tenant_id,
                r.principal.as_ref(),
                r.action.as_deref(),
                r.resource.as_ref(),
                r.purpose.as_deref(),
                r.elevation_id,
                outcome,
            ));
            outcome
        });
        Ok(outcome.decision())
    }

    async fn filter(&self, r: &FilterRequest) -> Result<Vec<ResourceKey>, DecisionUnavailable> {
        self.pause().await?;
        let now = Utc::now();
        Ok(self.with(|s| {
            let Some(subject) = subject_of(
                r.tenant_id,
                r.principal.as_ref(),
                r.action.as_deref(),
                r.purpose.as_deref(),
                r.elevation_id,
            ) else {
                return Vec::new();
            };
            let snap = Self::snapshot(
                s,
                subject.tenant_id,
                subject.principal,
                subject.elevation_id,
            );
            let mut permitted = Vec::new();
            for resource in &r.resources {
                let outcome = evaluate(&snap, &subject, resource, now);
                if filter_records(r.purpose.as_deref(), r.elevation_id, outcome) {
                    s.log.push(record_for(
                        r.tenant_id,
                        r.principal.as_ref(),
                        r.action.as_deref(),
                        Some(resource),
                        r.purpose.as_deref(),
                        r.elevation_id,
                        outcome,
                    ));
                }
                if outcome == Outcome::Permit {
                    permitted.push(ResourceKey {
                        resource_type: resource.resource_type.clone(),
                        id: resource.id.clone(),
                    });
                }
            }
            permitted
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    fn snapshot(role: Option<&str>) -> PolicySnapshot {
        PolicySnapshot {
            role: role.map(str::to_owned),
            grants: vec![Grant {
                role: "viewer".into(),
                resource_type: "catalog.service".into(),
                action: "read".into(),
                max_classification: Classification::Internal,
            }],
            purpose_rules: vec![PurposeRule {
                resource_type: "catalog.service".into(),
                action: "read".into(),
                min_classification: Classification::Internal,
            }],
            elevation: None,
        }
    }

    fn subject<'a>(
        p: &'a Principal,
        purpose: Option<&'a str>,
        elevation_id: Option<Uuid>,
    ) -> Subject<'a> {
        Subject {
            tenant_id: Uuid::nil(),
            principal: p,
            action: "read",
            purpose,
            elevation_id,
        }
    }

    #[test]
    fn classification_ceiling_and_purpose() {
        let p = Principal::service("viewer");
        let snap = snapshot(Some("viewer"));
        let public = ResourceRef::new("catalog.service", "a", Classification::Public);
        let internal = ResourceRef::new("catalog.service", "b", Classification::Internal);
        let restricted = ResourceRef::new("catalog.service", "c", Classification::Restricted);
        let now = Utc::now();
        assert_eq!(
            evaluate(&snap, &subject(&p, None, None), &public, now),
            Outcome::Permit
        );
        assert_eq!(
            evaluate(&snap, &subject(&p, None, None), &internal, now),
            Outcome::PurposeMissing
        );
        assert_eq!(
            evaluate(&snap, &subject(&p, Some("incident"), None), &internal, now),
            Outcome::Permit
        );
        assert_eq!(
            evaluate(
                &snap,
                &subject(&p, Some("incident"), None),
                &restricted,
                now
            ),
            Outcome::Deny(DenyReason::NoGrant)
        );
    }

    #[test]
    fn unknown_type_missing_classification_and_unknown_principal_are_denied() {
        let p = Principal::service("viewer");
        let now = Utc::now();
        let unknown_type = ResourceRef::new("nope", "a", Classification::Public);
        let unclassified = ResourceRef {
            classification: None,
            ..ResourceRef::new("catalog.service", "a", Classification::Public)
        };
        let public = ResourceRef::new("catalog.service", "a", Classification::Public);
        assert_eq!(
            evaluate(
                &snapshot(Some("viewer")),
                &subject(&p, None, None),
                &unknown_type,
                now
            )
            .decision(),
            Decision::Deny
        );
        assert_eq!(
            evaluate(
                &snapshot(Some("viewer")),
                &subject(&p, None, None),
                &unclassified,
                now
            )
            .decision(),
            Decision::Deny
        );
        assert_eq!(
            evaluate(&snapshot(None), &subject(&p, None, None), &public, now).decision(),
            Decision::Deny
        );
    }

    #[test]
    fn elevation_expiry_ownership_and_lift() {
        let user = Uuid::new_v4();
        let p = Principal::user(user);
        let id = Uuid::new_v4();
        let now = Utc::now();
        let restricted = ResourceRef::new("catalog.service", "c", Classification::Restricted);
        let mut snap = snapshot(Some("viewer"));
        snap.purpose_rules.clear();
        let elevation = Elevation {
            id,
            tenant_id: Uuid::nil(),
            user_id: user,
            resource_type: "catalog.service".into(),
            max_classification: Classification::Restricted,
            expires_at: now + ChronoDuration::minutes(5),
            revoked: false,
        };
        snap.elevation = Some(elevation.clone());
        assert_eq!(
            evaluate(&snap, &subject(&p, None, Some(id)), &restricted, now),
            Outcome::Permit
        );
        snap.elevation = Some(Elevation {
            expires_at: now - ChronoDuration::seconds(1),
            ..elevation.clone()
        });
        assert_eq!(
            evaluate(&snap, &subject(&p, None, Some(id)), &restricted, now),
            Outcome::Deny(DenyReason::ElevationExpired)
        );
        snap.elevation = Some(Elevation {
            revoked: true,
            ..elevation.clone()
        });
        assert_eq!(
            evaluate(&snap, &subject(&p, None, Some(id)), &restricted, now),
            Outcome::Deny(DenyReason::ElevationExpired)
        );
        snap.elevation = Some(Elevation {
            user_id: Uuid::new_v4(),
            ..elevation
        });
        assert_eq!(
            evaluate(&snap, &subject(&p, None, Some(id)), &restricted, now),
            Outcome::Deny(DenyReason::ElevationInvalid)
        );
    }

    #[test]
    fn incomplete_subject_is_rejected() {
        let p = Principal {
            kind: PrincipalKind::User,
            id: None,
            role: None,
        };
        assert!(subject_of(Some(Uuid::nil()), Some(&p), Some("read"), None, None).is_none());
        let s = Principal::service("viewer");
        assert!(subject_of(None, Some(&s), Some("read"), None, None).is_none());
        assert!(subject_of(Some(Uuid::nil()), None, Some("read"), None, None).is_none());
        assert!(subject_of(Some(Uuid::nil()), Some(&s), Some(""), None, None).is_none());
        assert!(subject_of(Some(Uuid::nil()), Some(&s), Some("read"), None, None).is_some());
    }
}
