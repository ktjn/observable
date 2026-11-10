//! Standalone harness: the health router with its real PostgreSQL schema (Testcontainers),
//! the provider-owned `decide-access` mock and substitutes for `validate-credential`, the
//! catalog reads and `store-telemetry`.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;

use axum::{Router, body::Body, http::Request};
use health_service::ports::{
    BindingInput, Caller, CatalogInputs, Credentials, Decider, EnvironmentClass, Evidence,
    ServiceInputs, ServiceReads, ServiceView,
};
use health_service::{HealthState, app};
use http_body_util::BodyExt as _;
use observable_auth::authz::{Classification, MockAccessDecider};
use serde_json::Value;
use tower::ServiceExt;
use uuid::Uuid;

pub struct Harness {
    pub app: Router,
    pub state: HealthState,
    pub decider: MockAccessDecider,
    pub services: ServiceReads,
    pub inputs: CatalogInputs,
    pub evidence: Evidence,
    pub tenant: Uuid,
    pub other_tenant: Uuid,
    /// Token -> user id of each human caller.
    pub users: HashMap<String, Uuid>,
    pub _db: test_support::postgres::TestDatabase,
}

/// Mirrors migrations 062, 067 and 069.
pub fn seed_default_policy(m: &MockAccessDecider) {
    use Classification::*;
    for (role, rt, max) in [
        ("viewer", "health.assessment", Public),
        ("member", "health.assessment", Public),
        ("tenant_admin", "health.assessment", Public),
        ("viewer", "health.evidence", Internal),
        ("member", "health.evidence", Confidential),
        ("tenant_admin", "health.evidence", Restricted),
        // Migration 067.
        ("viewer", "health.rule", Internal),
        ("member", "health.rule", Internal),
        ("tenant_admin", "health.rule", Internal),
    ] {
        m.grant(role, rt, "read", max);
    }
    m.grant("tenant_admin", "health.rule", "write", Internal);
    // Migration 069.
    for role in ["viewer", "member", "tenant_admin"] {
        m.grant(role, "health.maintenance", "read", Public);
    }
    m.grant("tenant_admin", "health.maintenance", "write", Restricted);
}

impl Harness {
    pub async fn start() -> Self {
        let (db, guard) = test_support::postgres::isolated_pool().await;
        let tenant = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let decider = MockAccessDecider::new();
        seed_default_policy(&decider);
        let mut callers = HashMap::new();
        let mut users = HashMap::new();
        for (token, t, role) in [
            ("member", tenant, "member"),
            ("viewer", tenant, "viewer"),
            ("admin", tenant, "tenant_admin"),
            ("admin2", tenant, "tenant_admin"),
            ("other-member", other_tenant, "member"),
        ] {
            let user = Uuid::new_v4();
            decider.add_member(t, user, role);
            users.insert(token.to_owned(), user);
            callers.insert(
                token.to_owned(),
                Caller {
                    tenant_id: t,
                    user_id: Some(user),
                    role: role.into(),
                },
            );
        }
        // An API key: a tenant_admin role without an accountable user.
        callers.insert(
            "admin-key".to_owned(),
            Caller {
                tenant_id: tenant,
                user_id: None,
                role: "tenant_admin".into(),
            },
        );
        let services = ServiceReads::substitute();
        let inputs = CatalogInputs::substitute();
        let evidence = Evidence::substitute();
        let state = HealthState::new(db).with_ports(
            Credentials::Static(Arc::new(callers)),
            Decider::Mock(decider.clone()),
            services.clone(),
            inputs.clone(),
            evidence.clone(),
        );
        Self {
            app: app(state.clone()),
            state,
            decider,
            services,
            inputs,
            evidence,
            tenant,
            other_tenant,
            users,
            _db: guard,
        }
    }

    pub async fn get(&self, uri: &str, token: Option<&str>) -> (u16, Value, String) {
        let mut req = Request::get(uri);
        if let Some(t) = token {
            req = req.header("Authorization", format!("Bearer {t}"));
        }
        let resp = self
            .app
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(bytes.to_vec()).unwrap();
        (
            status,
            serde_json::from_str(&raw).unwrap_or(Value::Null),
            raw,
        )
    }

    pub async fn send(
        &self,
        method: &str,
        uri: &str,
        token: &str,
        body: &str,
    ) -> (u16, Value, String) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8(bytes.to_vec()).unwrap();
        (
            status,
            serde_json::from_str(&raw).unwrap_or(Value::Null),
            raw,
        )
    }

    pub async fn health(&self, id: Uuid, class: &str, token: &str) -> (u16, Value, String) {
        self.get(
            &format!("/v1/health/services/{id}?environment_class={class}"),
            Some(token),
        )
        .await
    }

    pub async fn evaluate(&self) -> health_service::assessment::PassSummary {
        health_service::assessment::evaluate_all(&self.state.db, &self.inputs, &self.evidence).await
    }
}

pub fn view(id: Uuid, lifecycle: &str, classification: &str) -> ServiceView {
    ServiceView {
        id,
        lifecycle: lifecycle.into(),
        classification: classification.into(),
        owners: vec![],
    }
}

/// A service record as the worker reads it, bound to `name` in `prod` (production) and
/// `staging` (non-production).
pub fn inputs_for(id: Uuid, name: &str, lifecycle: &str) -> ServiceInputs {
    ServiceInputs {
        id,
        lifecycle: lifecycle.into(),
        criticality: "critical".into(),
        classification: "internal".into(),
        bindings: vec![
            BindingInput {
                service_name: name.into(),
                environment: "prod".into(),
                environment_class: Some(EnvironmentClass::Production),
            },
            BindingInput {
                service_name: name.into(),
                environment: "staging".into(),
                environment_class: Some(EnvironmentClass::NonProduction),
            },
        ],
    }
}

pub fn ns_ago(seconds: i64) -> u64 {
    (chrono::Utc::now() - chrono::Duration::seconds(seconds))
        .timestamp_nanos_opt()
        .unwrap() as u64
}
