//! Standalone harness: the catalog with its real PostgreSQL schema and the provider-owned
//! mocks of `validate-credential` (static callers), `decide-access` and `lookup-principals`, and
//! a substitute for the environment classes of `manage-control-plane`.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use axum::{Router, body::Body, http::Request};
use catalog_service::ports::{Caller, Credentials, Decider, Directory, Environments};
use catalog_service::{CatalogState, app};
use http_body_util::BodyExt as _;
use observable_auth::authz::{Classification, MockAccessDecider};
use observable_auth::principals::MockPrincipalDirectory;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

pub struct Harness {
    pub app: Router,
    pub decider: MockAccessDecider,
    pub directory: MockPrincipalDirectory,
    pub environments: Environments,
    pub tenant: Uuid,
    pub other_tenant: Uuid,
    pub admin: Uuid,
    pub member: Uuid,
    pub viewer: Uuid,
    pub _db: Option<test_support::postgres::TestDatabase>,
}

/// Mirrors the default grants of migrations 047, 049 and 052.
pub fn seed_default_policy(m: &MockAccessDecider) {
    use Classification::*;
    for (role, rt, action, max) in [
        ("viewer", "catalog.service", "read", Internal),
        ("member", "catalog.service", "read", Confidential),
        ("tenant_admin", "catalog.service", "read", Restricted),
        ("member", "catalog.service", "write", Internal),
        ("tenant_admin", "catalog.service", "write", Restricted),
        ("viewer", "catalog.team", "read", Internal),
        ("member", "catalog.team", "read", Internal),
        ("tenant_admin", "catalog.team", "read", Internal),
        ("member", "catalog.team", "write", Internal),
        ("tenant_admin", "catalog.team", "write", Internal),
        ("viewer", "catalog.journey", "read", Internal),
        ("member", "catalog.journey", "read", Confidential),
        ("tenant_admin", "catalog.journey", "read", Restricted),
        ("member", "catalog.journey", "write", Internal),
        ("tenant_admin", "catalog.journey", "write", Restricted),
    ] {
        m.grant(role, rt, action, max);
    }
}

impl Harness {
    pub async fn start() -> Self {
        let (db, guard) = test_support::postgres::isolated_pool().await;
        Self::with_db(db, Some(guard))
    }

    pub fn with_db(db: PgPool, guard: Option<test_support::postgres::TestDatabase>) -> Self {
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let (admin, member, viewer, outsider) = (
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
        );
        let decider = MockAccessDecider::new();
        seed_default_policy(&decider);
        let directory = MockPrincipalDirectory::new();
        for (user, role) in [
            (admin, "tenant_admin"),
            (member, "member"),
            (viewer, "viewer"),
        ] {
            decider.add_member(tenant, user, role);
            directory.add_member(tenant, user);
        }
        decider.add_member(other_tenant, outsider, "tenant_admin");
        directory.add_member(other_tenant, outsider);
        let caller = |t, u: Option<Uuid>, r: &str| Caller {
            tenant_id: t,
            user_id: u,
            role: r.into(),
        };
        let callers: HashMap<String, Caller> = HashMap::from([
            ("admin".into(), caller(tenant, Some(admin), "tenant_admin")),
            ("member".into(), caller(tenant, Some(member), "member")),
            ("viewer".into(), caller(tenant, Some(viewer), "viewer")),
            ("key-member".into(), caller(tenant, None, "member")),
            (
                "other-admin".into(),
                caller(other_tenant, Some(outsider), "tenant_admin"),
            ),
        ]);
        let environments = Environments::substitute();
        let state = CatalogState::new(
            db,
            Credentials::Static(Arc::new(callers)),
            Decider::Mock(decider.clone()),
            Directory::Mock(directory.clone()),
            environments.clone(),
        );
        Self {
            app: app(state),
            decider,
            directory,
            environments,
            tenant,
            other_tenant,
            admin,
            member,
            viewer,
            _db: guard,
        }
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<Value>,
    ) -> (u16, Value, String) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .expect("request");
        let resp = self.app.clone().oneshot(req).await.expect("router");
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.expect("body").to_bytes();
        let raw = String::from_utf8(bytes.to_vec()).expect("utf-8");
        (
            status,
            serde_json::from_str(&raw).unwrap_or(Value::Null),
            raw,
        )
    }

    /// Creates a service as tenant_admin and returns its id.
    pub async fn service(&self, body: Value) -> Uuid {
        let (status, json, raw) = self
            .call("POST", "/v1/catalog/services", Some("admin"), Some(body))
            .await;
        assert_eq!(status, 201, "{raw}");
        json["id"].as_str().expect("id").parse().expect("uuid")
    }
}
