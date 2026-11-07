//! Shared contract suites for `decide-access` and `lookup-principals`, run unchanged against
//! the real provider (observable-auth over PostgreSQL, through HTTP) and against the
//! provider-owned mocks (COMPONENT-ISOLATION ISO-04). Each side implements a harness that
//! seeds the fixture policy and exposes a decider whose policy store is unavailable.

use chrono::{Duration, Utc};
use uuid::Uuid;

use crate::authz::{
    AccessDecider, Classification, DecideRequest, Decision, DecisionRecord, DecisionUnavailable,
    Elevation, FilterRequest, MockAccessDecider, MockScenario, Principal, PrincipalKind,
    ResourceKey, ResourceRef,
};
use crate::principals::{
    DirectoryUnavailable, MockPrincipalDirectory, PrincipalDirectory, PrincipalState,
};

/// Resource type owned by the contract fixture, independent of product policy.
pub const RESOURCE_TYPE: &str = "contract.record";
pub const ACTION: &str = "read";

/// Fixture policy, seeded identically by both harnesses:
/// - `viewer` may read `contract.record` up to `confidential`;
/// - reading `contract.record` at `confidential` or above requires a purpose;
/// - `elevated` (a viewer) holds a valid elevation to `restricted` and an expired one.
#[derive(Clone, Debug)]
pub struct DecideFixture {
    pub tenant_id: Uuid,
    pub viewer: Uuid,
    pub outsider: Uuid,
    pub elevated: Uuid,
    pub valid_elevation: Uuid,
    pub expired_elevation: Uuid,
}

impl DecideFixture {
    pub fn random(tenant_id: Uuid) -> Self {
        Self {
            tenant_id,
            viewer: Uuid::new_v4(),
            outsider: Uuid::new_v4(),
            elevated: Uuid::new_v4(),
            valid_elevation: Uuid::new_v4(),
            expired_elevation: Uuid::new_v4(),
        }
    }

    pub fn elevations(&self) -> [Elevation; 2] {
        let base = Elevation {
            id: self.valid_elevation,
            tenant_id: self.tenant_id,
            user_id: self.elevated,
            resource_type: RESOURCE_TYPE.into(),
            max_classification: Classification::Restricted,
            expires_at: Utc::now() + Duration::hours(1),
            revoked: false,
        };
        let expired = Elevation {
            id: self.expired_elevation,
            expires_at: Utc::now() - Duration::hours(1),
            ..base.clone()
        };
        [base, expired]
    }

    fn request(
        &self,
        user: Uuid,
        classification: Option<Classification>,
        id: &str,
    ) -> DecideRequest {
        DecideRequest {
            tenant_id: Some(self.tenant_id),
            principal: Some(Principal::user(user)),
            action: Some(ACTION.into()),
            resource: Some(ResourceRef {
                resource_type: RESOURCE_TYPE.into(),
                id: id.into(),
                classification,
            }),
            purpose: None,
            elevation_id: None,
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait DecideAccessHarness {
    type Decider: AccessDecider;
    /// Tenant the fixture lives in (must exist on the provider).
    fn tenant_id(&self) -> Uuid;
    async fn seed(&self, fixture: &DecideFixture);
    fn decider(&self) -> Self::Decider;
    /// A decider whose policy store is unavailable.
    fn unavailable_decider(&self) -> Self::Decider;
    /// Decision-log records for the fixture tenant, oldest first.
    async fn decision_log(&self, tenant_id: Uuid) -> Vec<DecisionRecord>;
}

async fn fixture<H: DecideAccessHarness>(h: &H) -> DecideFixture {
    let f = DecideFixture::random(h.tenant_id());
    h.seed(&f).await;
    f
}

fn user_records(log: &[DecisionRecord], user: Uuid) -> Vec<DecisionRecord> {
    log.iter()
        .filter(|r| r.principal_id == Some(user))
        .cloned()
        .collect()
}

/// Acceptance: a permitted request returns permit and a record with actor, purpose,
/// elevation and decision.
pub async fn permit_is_recorded_with_actor_purpose_and_elevation<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let mut req = f.request(f.elevated, Some(Classification::Restricted), "r-1");
    req.purpose = Some("incident INC-1".into());
    req.elevation_id = Some(f.valid_elevation);
    assert_eq!(d.decide(&req).await, Ok(Decision::Permit));

    let records = user_records(&h.decision_log(f.tenant_id).await, f.elevated);
    let record = records.last().expect("permit must be recorded");
    assert_eq!(record.principal_kind, Some(PrincipalKind::User));
    assert_eq!(record.purpose.as_deref(), Some("incident INC-1"));
    assert_eq!(record.elevation_id, Some(f.valid_elevation));
    assert_eq!(record.resource_id.as_deref(), Some("r-1"));
    assert_eq!(record.decision, "permit");
}

/// Negative: a resource the principal may not see and a resource that does not exist (owner
/// sends no classification, or the type is unknown) produce the same decision.
pub async fn denied_and_nonexistent_are_indistinguishable<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let mut restricted = f.request(f.viewer, Some(Classification::Restricted), "exists");
    restricted.purpose = Some("audit".into());
    let mut missing = f.request(f.viewer, None, "does-not-exist");
    missing.purpose = Some("audit".into());
    let mut unknown_type = missing.clone();
    unknown_type.resource = Some(ResourceRef::new(
        "contract.unknown",
        "x",
        Classification::Public,
    ));
    let mut outsider = f.request(f.outsider, Some(Classification::Public), "exists");
    outsider.purpose = Some("audit".into());

    let outcomes = [
        d.decide(&restricted).await,
        d.decide(&missing).await,
        d.decide(&unknown_type).await,
        d.decide(&outsider).await,
    ];
    assert!(
        outcomes.iter().all(|o| *o == Ok(Decision::Deny)),
        "{outcomes:?}"
    );
}

/// Negative: a purpose-bound request without a stated purpose is denied and recorded.
pub async fn purpose_missing_is_denied_and_recorded<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let req = f.request(f.viewer, Some(Classification::Confidential), "pb-1");
    assert_eq!(d.decide(&req).await, Ok(Decision::PurposeMissing));
    let mut blank = req.clone();
    blank.purpose = Some("   ".into());
    assert_eq!(d.decide(&blank).await, Ok(Decision::PurposeMissing));

    let records = user_records(&h.decision_log(f.tenant_id).await, f.viewer);
    assert!(
        records
            .iter()
            .any(|r| r.decision == "purpose_missing" && r.resource_id.as_deref() == Some("pb-1")),
        "purpose-missing decision must be recorded: {records:?}"
    );

    let mut with_purpose = req;
    with_purpose.purpose = Some("customer complaint C-9".into());
    assert_eq!(d.decide(&with_purpose).await, Ok(Decision::Permit));
}

/// Negative: a request under an expired elevation grant is denied; a foreign grant too.
pub async fn expired_elevation_is_denied<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let mut req = f.request(f.elevated, Some(Classification::Restricted), "el-1");
    req.purpose = Some("break-glass".into());
    req.elevation_id = Some(f.expired_elevation);
    assert_eq!(d.decide(&req).await, Ok(Decision::Deny));

    // Even a resource the role could read without elevation is denied under an expired grant.
    let mut low = f.request(f.elevated, Some(Classification::Public), "el-2");
    low.elevation_id = Some(f.expired_elevation);
    assert_eq!(d.decide(&low).await, Ok(Decision::Deny));

    let mut foreign = f.request(f.viewer, Some(Classification::Restricted), "el-3");
    foreign.purpose = Some("break-glass".into());
    foreign.elevation_id = Some(f.valid_elevation);
    assert_eq!(d.decide(&foreign).await, Ok(Decision::Deny));

    let records = user_records(&h.decision_log(f.tenant_id).await, f.elevated);
    assert!(
        records
            .iter()
            .any(|r| r.elevation_id == Some(f.expired_elevation) && r.decision == "deny")
    );
}

/// Negative: when the policy store is unavailable, every request is denied.
pub async fn unavailable_store_denies_every_request<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.unavailable_decider();
    let mut req = f.request(f.viewer, Some(Classification::Public), "u-1");
    req.purpose = Some("any".into());
    assert_eq!(d.decide(&req).await, Err(DecisionUnavailable));
    let filter = FilterRequest {
        tenant_id: Some(f.tenant_id),
        principal: Some(Principal::user(f.viewer)),
        action: Some(ACTION.into()),
        resources: vec![ResourceRef::new(
            RESOURCE_TYPE,
            "u-1",
            Classification::Public,
        )],
        purpose: None,
        elevation_id: None,
    };
    assert_eq!(d.filter(&filter).await, Err(DecisionUnavailable));
}

/// List invariant: filter returns only the permitted subset, never denied ones.
pub async fn filter_returns_only_permitted_resources<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let resources = vec![
        ResourceRef::new(RESOURCE_TYPE, "public", Classification::Public),
        ResourceRef::new(RESOURCE_TYPE, "internal", Classification::Internal),
        ResourceRef::new(RESOURCE_TYPE, "confidential", Classification::Confidential),
        ResourceRef::new(RESOURCE_TYPE, "restricted", Classification::Restricted),
        ResourceRef {
            classification: None,
            ..ResourceRef::new(RESOURCE_TYPE, "missing", Classification::Public)
        },
    ];
    let mut req = FilterRequest {
        tenant_id: Some(f.tenant_id),
        principal: Some(Principal::user(f.viewer)),
        action: Some(ACTION.into()),
        resources,
        purpose: None,
        elevation_id: None,
    };
    let key = |id: &str| ResourceKey {
        resource_type: RESOURCE_TYPE.into(),
        id: id.into(),
    };
    assert_eq!(
        d.filter(&req).await,
        Ok(vec![key("public"), key("internal")])
    );

    req.purpose = Some("audit".into());
    assert_eq!(
        d.filter(&req).await,
        Ok(vec![key("public"), key("internal"), key("confidential")])
    );

    req.principal = Some(Principal::user(f.outsider));
    assert_eq!(d.filter(&req).await, Ok(vec![]));
}

/// Fail closed: a request without tenant, principal, action or resource is denied.
pub async fn incomplete_request_is_denied<H: DecideAccessHarness>(h: &H) {
    let f = fixture(h).await;
    let d = h.decider();
    let full = f.request(f.viewer, Some(Classification::Public), "i-1");
    assert_eq!(d.decide(&full).await, Ok(Decision::Permit));
    for strip in 0..4 {
        let mut req = full.clone();
        match strip {
            0 => req.tenant_id = None,
            1 => req.principal = None,
            2 => req.action = None,
            _ => req.resource = None,
        }
        assert_eq!(d.decide(&req).await, Ok(Decision::Deny), "strip {strip}");
    }
    let mut service_without_role = full.clone();
    service_without_role.principal = Some(Principal {
        kind: PrincipalKind::Service,
        id: None,
        role: None,
    });
    assert_eq!(d.decide(&service_without_role).await, Ok(Decision::Deny));
}

/// Runs every `decide-access` contract case.
pub async fn run_decide_access_suite<H: DecideAccessHarness>(h: &H) {
    permit_is_recorded_with_actor_purpose_and_elevation(h).await;
    denied_and_nonexistent_are_indistinguishable(h).await;
    purpose_missing_is_denied_and_recorded(h).await;
    expired_elevation_is_denied(h).await;
    unavailable_store_denies_every_request(h).await;
    filter_returns_only_permitted_resources(h).await;
    incomplete_request_is_denied(h).await;
}

/// Mock-side harness: seeds [`MockAccessDecider`] with the fixture policy.
pub struct MockDecideHarness {
    pub mock: MockAccessDecider,
    pub tenant_id: Uuid,
}

impl Default for MockDecideHarness {
    fn default() -> Self {
        Self {
            mock: MockAccessDecider::new(),
            tenant_id: Uuid::new_v4(),
        }
    }
}

impl DecideAccessHarness for MockDecideHarness {
    type Decider = MockAccessDecider;

    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    async fn seed(&self, f: &DecideFixture) {
        let m = &self.mock;
        m.grant(
            "viewer",
            RESOURCE_TYPE,
            ACTION,
            Classification::Confidential,
        );
        m.require_purpose(RESOURCE_TYPE, ACTION, Classification::Confidential);
        m.add_member(f.tenant_id, f.viewer, "viewer");
        m.add_member(f.tenant_id, f.elevated, "viewer");
        for e in f.elevations() {
            m.add_elevation(e);
        }
    }

    fn decider(&self) -> MockAccessDecider {
        self.mock.clone()
    }

    fn unavailable_decider(&self) -> MockAccessDecider {
        let m = MockAccessDecider::new();
        m.set_scenario(MockScenario::Unavailable);
        m
    }

    async fn decision_log(&self, tenant_id: Uuid) -> Vec<DecisionRecord> {
        self.mock
            .decision_log()
            .into_iter()
            .filter(|r| r.tenant_id == Some(tenant_id))
            .collect()
    }
}

/// `lookup-principals` fixture: one member, one known user without membership, one unknown id.
#[derive(Clone, Debug)]
pub struct PrincipalsFixture {
    pub tenant_id: Uuid,
    pub active: Uuid,
    pub departed: Uuid,
    pub unknown: Uuid,
}

#[allow(async_fn_in_trait)]
pub trait LookupPrincipalsHarness {
    type Directory: PrincipalDirectory;
    fn tenant_id(&self) -> Uuid;
    async fn seed(&self, fixture: &PrincipalsFixture);
    fn directory(&self) -> Self::Directory;
    fn unavailable_directory(&self) -> Self::Directory;
}

/// Active, departed and unknown principals resolve in request order, duplicates collapsed.
pub async fn principals_resolve_active_departed_unknown<H: LookupPrincipalsHarness>(h: &H) {
    let f = PrincipalsFixture {
        tenant_id: h.tenant_id(),
        active: Uuid::new_v4(),
        departed: Uuid::new_v4(),
        unknown: Uuid::new_v4(),
    };
    h.seed(&f).await;
    let got = h
        .directory()
        .resolve(f.tenant_id, &[f.unknown, f.active, f.departed, f.active])
        .await
        .expect("directory available");
    let states: Vec<_> = got.iter().map(|p| (p.user_id, p.state)).collect();
    assert_eq!(
        states,
        vec![
            (f.unknown, PrincipalState::Unknown),
            (f.active, PrincipalState::Active),
            (f.departed, PrincipalState::Departed),
        ]
    );
    // Membership is tenant-scoped: an active member elsewhere is departed here.
    let other = h
        .directory()
        .resolve(Uuid::new_v4(), &[f.active])
        .await
        .expect("available");
    assert_eq!(other[0].state, PrincipalState::Departed);
}

pub async fn principals_unavailable_fails<H: LookupPrincipalsHarness>(h: &H) {
    assert_eq!(
        h.unavailable_directory()
            .resolve(h.tenant_id(), &[Uuid::new_v4()])
            .await,
        Err(DirectoryUnavailable)
    );
}

pub async fn run_lookup_principals_suite<H: LookupPrincipalsHarness>(h: &H) {
    principals_resolve_active_departed_unknown(h).await;
    principals_unavailable_fails(h).await;
}

pub struct MockPrincipalsHarness {
    pub mock: MockPrincipalDirectory,
    pub tenant_id: Uuid,
}

impl Default for MockPrincipalsHarness {
    fn default() -> Self {
        Self {
            mock: MockPrincipalDirectory::new(),
            tenant_id: Uuid::new_v4(),
        }
    }
}

impl LookupPrincipalsHarness for MockPrincipalsHarness {
    type Directory = MockPrincipalDirectory;

    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    async fn seed(&self, f: &PrincipalsFixture) {
        self.mock.add_member(f.tenant_id, f.active);
        self.mock.add_user(f.departed);
    }

    fn directory(&self) -> MockPrincipalDirectory {
        self.mock.clone()
    }

    fn unavailable_directory(&self) -> MockPrincipalDirectory {
        let m = MockPrincipalDirectory::new();
        m.set_scenario(crate::principals::MockScenario::Unavailable);
        m
    }
}
