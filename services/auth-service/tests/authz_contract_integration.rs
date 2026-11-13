//! `decide-access` and `lookup-principals` contract suites against the real provider:
//! auth-service's router over a migrated PostgreSQL (Testcontainers or the external test
//! server), called through the consumer HTTP adapters. The same suites run against the
//! provider-owned mocks in libs/observable-auth/tests/contract_mock.rs.

use std::sync::Arc;
use std::time::Duration;

use auth_service::authz::{AuthzState, router};
use observable_auth::authz::{DecisionRecord, HttpAccessDecider, PrincipalKind};
use observable_auth::contract::{
    self, ACTION, DecideAccessHarness, DecideFixture, LookupPrincipalsHarness, PrincipalsFixture,
    RESOURCE_TYPE,
};
use observable_auth::principals::HttpPrincipalDirectory;
use serde_json::{Value, json};
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use uuid::Uuid;

const TOKEN: &str = "contract-internal-token";

async fn serve(db: PgPool) -> String {
    let app = router(AuthzState {
        db,
        internal_token: Arc::new(TOKEN.into()),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{addr}")
}

fn unreachable_pool() -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(Duration::from_millis(300))
        .connect_lazy("postgres://observable_auth:observable_auth@127.0.0.1:1/observable")
        .expect("lazy pool")
}

struct RealProvider {
    db: PgPool,
    url: String,
    down_url: String,
    tenant_id: Uuid,
    _guard: test_support::postgres::TestDatabase,
}

impl RealProvider {
    async fn start() -> Self {
        let (db, guard) = test_support::postgres::isolated_pool().await;
        let tenant_id = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2)")
            .bind(tenant_id)
            .bind(format!("contract-{tenant_id}"))
            .execute(&db)
            .await
            .expect("tenant");
        let url = serve(db.clone()).await;
        let down_url = serve(unreachable_pool()).await;
        Self {
            db,
            url,
            down_url,
            tenant_id,
            _guard: guard,
        }
    }

    async fn add_user(&self, id: Uuid) {
        sqlx::query("INSERT INTO auth.users (id, idp_subject, email) VALUES ($1, $2, $3)")
            .bind(id)
            .bind(format!("sub-{id}"))
            .bind(format!("{id}@example.test"))
            .execute(&self.db)
            .await
            .expect("user");
    }

    async fn add_member(&self, tenant_id: Uuid, id: Uuid, role: &str) {
        sqlx::query(
            "INSERT INTO auth.user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(tenant_id)
        .bind(role)
        .execute(&self.db)
        .await
        .expect("membership");
    }

    async fn post(&self, base: &str, path: &str, token: Option<&str>, body: &str) -> (u16, String) {
        let mut req = reqwest::Client::new()
            .post(format!("{base}{path}"))
            .header("content-type", "application/json")
            .body(body.to_owned());
        if let Some(t) = token {
            req = req.header("X-Internal-Token", t);
        }
        let resp = req.send().await.expect("request");
        (resp.status().as_u16(), resp.text().await.expect("body"))
    }
}

impl DecideAccessHarness for RealProvider {
    type Decider = HttpAccessDecider;

    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    async fn seed(&self, f: &DecideFixture) {
        sqlx::query(
            "INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) \
             VALUES ($1, 'viewer', $2, $3, 'confidential') ON CONFLICT DO NOTHING",
        )
        .bind(f.tenant_id)
        .bind(RESOURCE_TYPE)
        .bind(ACTION)
        .execute(&self.db)
        .await
        .expect("grant");
        sqlx::query(
            "INSERT INTO auth.authz_purpose_rules (tenant_id, resource_type, action, min_classification) \
             VALUES ($1, $2, $3, 'confidential') ON CONFLICT DO NOTHING",
        )
        .bind(f.tenant_id)
        .bind(RESOURCE_TYPE)
        .bind(ACTION)
        .execute(&self.db)
        .await
        .expect("purpose rule");
        for user in [f.viewer, f.outsider, f.elevated] {
            self.add_user(user).await;
        }
        self.add_member(f.tenant_id, f.viewer, "viewer").await;
        self.add_member(f.tenant_id, f.elevated, "viewer").await;
        for e in f.elevations() {
            sqlx::query(
                "INSERT INTO auth.authz_elevations (id, tenant_id, user_id, resource_type, max_classification, \
                 justification, granted_at, expires_at) VALUES ($1, $2, $3, $4, $5, 'contract', $6, $7)",
            )
            .bind(e.id)
            .bind(e.tenant_id)
            .bind(e.user_id)
            .bind(&e.resource_type)
            .bind(e.max_classification.as_str())
            .bind(e.expires_at - chrono::Duration::hours(2))
            .bind(e.expires_at)
            .execute(&self.db)
            .await
            .expect("elevation");
        }
    }

    fn decider(&self) -> HttpAccessDecider {
        HttpAccessDecider::new(&self.url, TOKEN, reqwest::Client::new())
    }

    fn unavailable_decider(&self) -> HttpAccessDecider {
        HttpAccessDecider::new(&self.down_url, TOKEN, reqwest::Client::new())
    }

    async fn decision_log(&self, tenant_id: Uuid) -> Vec<DecisionRecord> {
        sqlx::query(
            "SELECT tenant_id, principal_kind, principal_id, resource_type, resource_id, action, \
             purpose, elevation_id, decision, reason FROM auth.access_decision_log \
             WHERE tenant_id = $1 ORDER BY id",
        )
        .bind(tenant_id)
        .fetch_all(&self.db)
        .await
        .expect("decision log")
        .into_iter()
        .map(|r| DecisionRecord {
            tenant_id: r.get("tenant_id"),
            principal_kind: r.get::<Option<String>, _>("principal_kind").map(|k| {
                match k.as_str() {
                    "user" => PrincipalKind::User,
                    _ => PrincipalKind::Service,
                }
            }),
            principal_id: r.get("principal_id"),
            resource_type: r.get("resource_type"),
            resource_id: r.get("resource_id"),
            action: r.get("action"),
            purpose: r.get("purpose"),
            elevation_id: r.get("elevation_id"),
            decision: r.get("decision"),
            reason: r.get("reason"),
        })
        .collect()
    }
}

impl LookupPrincipalsHarness for RealProvider {
    type Directory = HttpPrincipalDirectory;

    fn tenant_id(&self) -> Uuid {
        self.tenant_id
    }

    async fn seed(&self, f: &PrincipalsFixture) {
        self.add_user(f.active).await;
        self.add_user(f.departed).await;
        self.add_member(f.tenant_id, f.active, "member").await;
    }

    fn directory(&self) -> HttpPrincipalDirectory {
        HttpPrincipalDirectory::new(&self.url, TOKEN, reqwest::Client::new())
    }

    fn unavailable_directory(&self) -> HttpPrincipalDirectory {
        HttpPrincipalDirectory::new(&self.down_url, TOKEN, reqwest::Client::new())
    }
}

#[tokio::test]
async fn real_permit_is_recorded_with_actor_purpose_and_elevation() {
    contract::permit_is_recorded_with_actor_purpose_and_elevation(&RealProvider::start().await)
        .await;
}

#[tokio::test]
async fn real_denied_and_nonexistent_are_indistinguishable() {
    contract::denied_and_nonexistent_are_indistinguishable(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_purpose_missing_is_denied_and_recorded() {
    contract::purpose_missing_is_denied_and_recorded(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_expired_elevation_is_denied() {
    contract::expired_elevation_is_denied(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_unavailable_store_denies_every_request() {
    contract::unavailable_store_denies_every_request(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_filter_returns_only_permitted_resources() {
    contract::filter_returns_only_permitted_resources(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_incomplete_request_is_denied() {
    contract::incomplete_request_is_denied(&RealProvider::start().await).await;
}

#[tokio::test]
async fn real_lookup_principals_suite() {
    contract::run_lookup_principals_suite(&RealProvider::start().await).await;
}

/// Wire-level existence hiding: a restricted resource, an unclassified (non-existent)
/// resource, an unknown resource type, a non-member principal and a malformed body all get
/// byte-identical responses.
#[tokio::test]
async fn real_denied_and_nonexistent_responses_are_byte_identical() {
    let p = RealProvider::start().await;
    let f = DecideFixture::random(p.tenant_id);
    DecideAccessHarness::seed(&p, &f).await;
    let body = |user: Uuid, resource: Value| {
        json!({
            "tenant_id": f.tenant_id,
            "principal": {"kind": "user", "id": user},
            "action": ACTION,
            "resource": resource,
            "purpose": "audit",
        })
        .to_string()
    };
    let cases = [
        body(
            f.viewer,
            json!({"resource_type": RESOURCE_TYPE, "id": "exists", "classification": "restricted"}),
        ),
        body(
            f.viewer,
            json!({"resource_type": RESOURCE_TYPE, "id": "missing"}),
        ),
        body(
            f.viewer,
            json!({"resource_type": "contract.unknown", "id": "x", "classification": "public"}),
        ),
        body(
            f.outsider,
            json!({"resource_type": RESOURCE_TYPE, "id": "exists", "classification": "public"}),
        ),
        "{not json".to_owned(),
    ];
    let mut responses = Vec::new();
    for c in &cases {
        responses.push(
            p.post(&p.url, "/internal/authz/decide", Some(TOKEN), c)
                .await,
        );
    }
    assert!(
        responses
            .iter()
            .all(|r| *r == (403, r#"{"decision":"deny"}"#.to_owned())),
        "{responses:?}"
    );
}

#[tokio::test]
async fn real_internal_token_is_required() {
    let p = RealProvider::start().await;
    for path in [
        "/internal/authz/decide",
        "/internal/authz/filter",
        "/internal/principals/resolve",
    ] {
        assert_eq!(p.post(&p.url, path, None, "{}").await.0, 401, "{path}");
        assert_eq!(
            p.post(&p.url, path, Some("wrong"), "{}").await.0,
            401,
            "{path}"
        );
    }
}

#[tokio::test]
async fn real_unavailable_store_returns_decision_unavailable() {
    let p = RealProvider::start().await;
    let body = json!({
        "tenant_id": p.tenant_id,
        "principal": {"kind": "service", "role": "tenant_admin"},
        "action": "read",
        "resource": {"resource_type": "catalog.service", "id": "s", "classification": "public"},
    })
    .to_string();
    assert_eq!(
        p.post(&p.down_url, "/internal/authz/decide", Some(TOKEN), &body)
            .await,
        (503, r#"{"error":"decision_unavailable"}"#.to_owned())
    );
}

#[tokio::test]
async fn default_catalog_grants_follow_role_ceilings() {
    let p = RealProvider::start().await;
    let d = HttpAccessDecider::new(&p.url, TOKEN, reqwest::Client::new());
    use observable_auth::authz::{
        AccessDecider, Classification, FilterRequest, Principal, ResourceRef,
    };
    let resources: Vec<_> = [
        Classification::Public,
        Classification::Internal,
        Classification::Confidential,
        Classification::Restricted,
    ]
    .into_iter()
    .map(|c| ResourceRef::new("catalog.service", c.as_str(), c))
    .collect();
    for (role, visible) in [("viewer", 2), ("member", 3), ("tenant_admin", 4)] {
        let req = FilterRequest {
            tenant_id: Some(p.tenant_id),
            principal: Some(Principal::service(role)),
            action: Some("read".into()),
            resources: resources.clone(),
            purpose: None,
            elevation_id: None,
        };
        assert_eq!(
            d.filter(&req).await.expect("available").len(),
            visible,
            "{role}"
        );
    }
}

#[tokio::test]
async fn health_worker_grant_reads_catalog_services_and_writes_nothing() {
    let p = RealProvider::start().await;
    let d = HttpAccessDecider::new(&p.url, TOKEN, reqwest::Client::new());
    use observable_auth::authz::{
        AccessDecider, Classification, DecideRequest, Decision, FilterRequest, Principal,
        ResourceRef,
    };
    let all = [
        Classification::Public,
        Classification::Internal,
        Classification::Confidential,
        Classification::Restricted,
    ];
    let resources: Vec<_> = all
        .into_iter()
        .map(|c| ResourceRef::new("catalog.service", c.as_str(), c))
        .collect();
    let filter = |action: &str, resources: Vec<ResourceRef>| FilterRequest {
        tenant_id: Some(p.tenant_id),
        principal: Some(Principal::service("health_worker")),
        action: Some(action.into()),
        resources,
        purpose: None,
        elevation_id: None,
    };
    assert_eq!(
        d.filter(&filter("read", resources.clone()))
            .await
            .unwrap()
            .len(),
        4
    );
    assert!(
        d.filter(&filter("write", resources))
            .await
            .unwrap()
            .is_empty()
    );
    // No grant reaches other catalog records or other providers' resources.
    for rt in ["catalog.journey", "catalog.provider", "catalog.team"] {
        let r = vec![ResourceRef::new(rt, "x", Classification::Public)];
        assert!(
            d.filter(&filter("read", r)).await.unwrap().is_empty(),
            "{rt}"
        );
    }
    let decide = DecideRequest {
        tenant_id: Some(p.tenant_id),
        principal: Some(Principal::service("health_worker")),
        action: Some("write".into()),
        resource: Some(ResourceRef::new(
            "catalog.service",
            "svc",
            Classification::Public,
        )),
        purpose: None,
        elevation_id: None,
    };
    assert!(matches!(d.decide(&decide).await, Ok(Decision::Deny)));
}

#[tokio::test]
async fn default_health_grants_follow_role_ceilings() {
    let p = RealProvider::start().await;
    let d = HttpAccessDecider::new(&p.url, TOKEN, reqwest::Client::new());
    use observable_auth::authz::{
        AccessDecider, Classification, FilterRequest, Principal, ResourceRef,
    };
    let all = [
        Classification::Public,
        Classification::Internal,
        Classification::Confidential,
        Classification::Restricted,
    ];
    let of = |rt: &str| -> Vec<ResourceRef> {
        all.into_iter()
            .map(|c| ResourceRef::new(rt, c.as_str(), c))
            .collect()
    };
    for (role, evidence, assessment) in [
        ("viewer", 2, 1),
        ("member", 3, 1),
        ("tenant_admin", 4, 1),
        ("health_worker", 0, 0),
    ] {
        for (rt, visible) in [
            ("health.evidence", evidence),
            ("health.assessment", assessment),
            // Migration 071: history is request-level, like the assessment.
            ("health.history", assessment),
            // Migration 073: issues are request-level too.
            ("health.issue", assessment),
        ] {
            let req = FilterRequest {
                tenant_id: Some(p.tenant_id),
                principal: Some(Principal::service(role)),
                action: Some("read".into()),
                resources: of(rt),
                purpose: None,
                elevation_id: None,
            };
            assert_eq!(
                d.filter(&req).await.expect("available").len(),
                visible,
                "{role} {rt}"
            );
        }
    }
}

#[tokio::test]
async fn default_health_rule_grants_let_only_tenant_admin_publish() {
    let p = RealProvider::start().await;
    let d = HttpAccessDecider::new(&p.url, TOKEN, reqwest::Client::new());
    use observable_auth::authz::{
        AccessDecider, Classification, DecideRequest, Decision, Principal, ResourceRef,
    };
    for (role, read, write) in [
        ("viewer", true, false),
        ("member", true, false),
        ("tenant_admin", true, true),
        ("health_worker", false, false),
    ] {
        for (action, expected) in [("read", read), ("write", write)] {
            let req = DecideRequest {
                tenant_id: Some(p.tenant_id),
                principal: Some(Principal::service(role)),
                action: Some(action.into()),
                resource: Some(ResourceRef::new(
                    "health.rule",
                    "current",
                    Classification::Internal,
                )),
                purpose: None,
                elevation_id: None,
            };
            let permitted = matches!(d.decide(&req).await, Ok(Decision::Permit));
            assert_eq!(permitted, expected, "{role} {action}");
        }
    }
}

#[tokio::test]
async fn default_health_maintenance_grants_let_only_tenant_admin_write() {
    let p = RealProvider::start().await;
    let d = HttpAccessDecider::new(&p.url, TOKEN, reqwest::Client::new());
    use observable_auth::authz::{
        AccessDecider, Classification, DecideRequest, Decision, Principal, ResourceRef,
    };
    for (role, read, write) in [
        ("viewer", true, false),
        ("member", true, false),
        ("tenant_admin", true, true),
        ("health_worker", false, false),
    ] {
        for (action, class, expected) in [
            ("read", Classification::Public, read),
            ("write", Classification::Restricted, write),
        ] {
            let req = DecideRequest {
                tenant_id: Some(p.tenant_id),
                principal: Some(Principal::service(role)),
                action: Some(action.into()),
                resource: Some(ResourceRef::new("health.maintenance", "svc", class)),
                purpose: None,
                elevation_id: None,
            };
            let permitted = matches!(d.decide(&req).await, Ok(Decision::Permit));
            assert_eq!(permitted, expected, "{role} {action}");
        }
    }
}
