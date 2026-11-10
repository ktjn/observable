// Auth-served tenant administration (/v1/tokens, /v1/admin/members) under the scoped
// observable_auth login: tenant_admin only (current membership, not the login-time role),
// tenant from the verified session, mutations audited in the same transaction.

use std::sync::Arc;

use auth_service::{
    observability::AuthServiceMetrics,
    oidc::{OidcConfig, OidcState},
    session::sign_session_jwt,
    tenant_admin,
    validate::sha256_hex,
};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

const SECRET: &str = "tenant-admin-test-secret-0123456789";

struct Harness {
    owner: PgPool,
    app: Router,
    _guard: test_support::postgres::TestDatabase,
}

impl Harness {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        let options = owner
            .connect_options()
            .as_ref()
            .clone()
            .username("observable_auth")
            .password("observable_auth");
        let scoped = PgPoolOptions::new().connect_with(options).await.unwrap();
        let state = OidcState {
            db: scoped,
            config: OidcConfig {
                issuer: "http://127.0.0.1:1".into(),
                api_base: "http://127.0.0.1:1".into(),
                client_id: "test".into(),
                redirect_uri: "http://localhost/cb".into(),
                session_secret: SECRET.into(),
                dev_mode: false,
            },
            http_client: reqwest::Client::new(),
            metrics: Arc::new(AuthServiceMetrics::new()),
        };
        Self {
            owner,
            app: tenant_admin::router().with_state(state),
            _guard: guard,
        }
    }

    async fn user(&self, email: &str) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO auth.users (idp_subject, email, name) VALUES ($1, $1, 'N') RETURNING id",
        )
        .bind(email)
        .fetch_one(&self.owner)
        .await
        .unwrap()
    }

    async fn member(&self, user: Uuid, tenant: Uuid, role: &str) {
        sqlx::query(
            "INSERT INTO auth.user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, $3) \
             ON CONFLICT (user_id, tenant_id) DO UPDATE SET role = EXCLUDED.role",
        )
        .bind(user)
        .bind(tenant)
        .bind(role)
        .execute(&self.owner)
        .await
        .unwrap();
    }

    /// An active session for `user` in `tenant`, with `role` captured at login.
    async fn session(&self, user: Uuid, tenant: Uuid, role: &str) -> String {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO auth.user_sessions (user_id, tenant_id, environment, expires_at) \
             VALUES ($1, $2, 'prod', now() + interval '1 hour') RETURNING id",
        )
        .bind(user)
        .bind(tenant)
        .fetch_one(&self.owner)
        .await
        .unwrap();
        sign_session_jwt(SECRET, user, tenant, role, "prod", id).unwrap()
    }

    /// A tenant with an admin and their session.
    async fn tenant_with_admin(&self) -> (Uuid, Uuid, String) {
        let tenant = Uuid::new_v4();
        let admin = self.user(&format!("admin-{tenant}@example.com")).await;
        self.member(admin, tenant, "tenant_admin").await;
        let session = self.session(admin, tenant, "tenant_admin").await;
        (tenant, admin, session)
    }

    async fn call(
        &self,
        method: &str,
        uri: &str,
        session: Option<&str>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(s) = session {
            req = req.header("cookie", format!("session={s}"));
        }
        let req = match body {
            Some(b) => req
                .header("content-type", "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn audit_actions(&self, tenant: Uuid) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT action FROM auth.credential_audit_log WHERE tenant_id = $1 ORDER BY occurred_at, action",
        )
        .bind(tenant)
        .fetch_all(&self.owner)
        .await
        .unwrap()
    }
}

#[tokio::test]
async fn token_lifecycle_is_tenant_scoped_hashed_and_audited() {
    let h = Harness::start().await;
    let (tenant, _, session) = h.tenant_with_admin().await;

    let (status, created) = h
        .call(
            "POST",
            "/v1/tokens",
            Some(&session),
            Some(json!({"name": " ci ", "environment": " prod "})),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let plaintext = created["plaintext"].as_str().unwrap().to_owned();
    assert_eq!(plaintext.len(), 64);
    assert_eq!(created["name"], "ci");
    assert_eq!(created["environment"], "prod");
    assert_eq!(created["revoked"], false);
    let id = created["id"].as_str().unwrap().to_owned();
    let stored: String =
        sqlx::query_scalar("SELECT key_hash FROM auth.api_keys WHERE id = $1::uuid")
            .bind(&id)
            .fetch_one(&h.owner)
            .await
            .unwrap();
    assert_eq!(stored, sha256_hex(&plaintext));

    // Active tokens are neither restorable nor deletable.
    assert_eq!(
        h.call(
            "POST",
            &format!("/v1/tokens/{id}/restore"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        h.call(
            "DELETE",
            &format!("/v1/tokens/{id}/permanent"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        h.call("DELETE", &format!("/v1/tokens/{id}"), Some(&session), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    let (_, list) = h.call("GET", "/v1/tokens", Some(&session), None).await;
    assert_eq!(list["tokens"][0]["revoked"], true);
    assert_eq!(
        h.call(
            "POST",
            &format!("/v1/tokens/{id}/restore"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );

    let (status, renewed) = h
        .call(
            "POST",
            &format!("/v1/tokens/{id}/renew"),
            Some(&session),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(renewed["plaintext"], created["plaintext"]);
    assert_eq!(renewed["revoked"], false);

    assert_eq!(
        h.call("DELETE", &format!("/v1/tokens/{id}"), Some(&session), None)
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        h.call(
            "DELETE",
            &format!("/v1/tokens/{id}/permanent"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (_, list) = h.call("GET", "/v1/tokens", Some(&session), None).await;
    assert_eq!(list, json!({"tokens": []}));

    let mut actions = h.audit_actions(tenant).await;
    actions.sort();
    assert_eq!(
        actions,
        [
            "token_create",
            "token_delete",
            "token_renew",
            "token_restore",
            "token_revoke",
            "token_revoke"
        ]
    );
}

#[tokio::test]
async fn tokens_of_another_tenant_are_not_found_and_input_is_validated() {
    let h = Harness::start().await;
    let (_, _, session_a) = h.tenant_with_admin().await;
    let (_, _, session_b) = h.tenant_with_admin().await;
    let (_, created) = h
        .call(
            "POST",
            "/v1/tokens",
            Some(&session_a),
            Some(json!({"name": "a", "environment": "prod"})),
        )
        .await;
    let id = created["id"].as_str().unwrap();

    let (_, list) = h.call("GET", "/v1/tokens", Some(&session_b), None).await;
    assert_eq!(list, json!({"tokens": []}));
    for (method, uri) in [
        ("DELETE", format!("/v1/tokens/{id}")),
        ("POST", format!("/v1/tokens/{id}/renew")),
    ] {
        assert_eq!(
            h.call(method, &uri, Some(&session_b), None).await.0,
            StatusCode::NOT_FOUND
        );
    }
    for body in [
        json!({"name": " ", "environment": "prod"}),
        json!({"name": "x", "environment": ""}),
    ] {
        assert_eq!(
            h.call("POST", "/v1/tokens", Some(&session_a), Some(body))
                .await
                .0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}

#[tokio::test]
async fn only_current_tenant_admins_reach_admin_routes() {
    let h = Harness::start().await;
    let (tenant, _, _) = h.tenant_with_admin().await;
    assert_eq!(
        h.call("GET", "/v1/tokens", None, None).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        h.call("GET", "/v1/tokens", Some("not-a-jwt"), None).await.0,
        StatusCode::UNAUTHORIZED
    );

    let member = h.user("member@example.com").await;
    h.member(member, tenant, "member").await;
    let s = h.session(member, tenant, "member").await;
    assert_eq!(
        h.call("GET", "/v1/admin/members", Some(&s), None).await.0,
        StatusCode::FORBIDDEN
    );

    // Demoted after login: the session still claims tenant_admin, the membership decides.
    let demoted = h.user("demoted@example.com").await;
    h.member(demoted, tenant, "tenant_admin").await;
    let s = h.session(demoted, tenant, "tenant_admin").await;
    assert_eq!(
        h.call("GET", "/v1/tokens", Some(&s), None).await.0,
        StatusCode::OK
    );
    h.member(demoted, tenant, "viewer").await;
    assert_eq!(
        h.call("GET", "/v1/tokens", Some(&s), None).await.0,
        StatusCode::FORBIDDEN
    );

    // A revoked session is refused.
    sqlx::query("UPDATE auth.user_sessions SET revoked_at = now() WHERE user_id = $1")
        .bind(demoted)
        .execute(&h.owner)
        .await
        .unwrap();
    assert_eq!(
        h.call("GET", "/v1/tokens", Some(&s), None).await.0,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn members_are_managed_within_the_session_tenant() {
    let h = Harness::start().await;
    let (tenant, admin, session) = h.tenant_with_admin().await;
    let alice = h.user("alice@example.com").await;

    assert_eq!(
        h.call(
            "POST",
            "/v1/admin/members",
            Some(&session),
            Some(json!({"email": "nobody@example.com", "role": "member"}))
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        h.call(
            "POST",
            "/v1/admin/members",
            Some(&session),
            Some(json!({"email": "alice@example.com", "role": "owner"}))
        )
        .await
        .0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let (status, added) = h
        .call(
            "POST",
            "/v1/admin/members",
            Some(&session),
            Some(json!({"email": "alice@example.com", "role": "member"})),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(added["user_id"], alice.to_string());
    assert_eq!(added["role"], "member");

    let (_, list) = h
        .call("GET", "/v1/admin/members", Some(&session), None)
        .await;
    let ids: Vec<&str> = list["members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["user_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [admin.to_string(), alice.to_string()]);

    assert_eq!(
        h.call(
            "PUT",
            &format!("/v1/admin/members/{alice}/role"),
            Some(&session),
            Some(json!({"role": "viewer"}))
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        h.call(
            "PUT",
            &format!("/v1/admin/members/{admin}/role"),
            Some(&session),
            Some(json!({"role": "viewer"}))
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );

    let alice_session = h.session(alice, tenant, "viewer").await;
    assert_eq!(
        h.call(
            "POST",
            &format!("/v1/admin/members/{alice}/revoke-sessions"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM auth.user_sessions WHERE user_id = $1 AND revoked_at IS NULL",
    )
    .bind(alice)
    .fetch_one(&h.owner)
    .await
    .unwrap();
    assert_eq!(active, 0);
    drop(alice_session);

    h.session(alice, tenant, "viewer").await;
    assert_eq!(
        h.call(
            "DELETE",
            &format!("/v1/admin/members/{alice}"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let (remaining, active): (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM auth.user_tenant_roles WHERE user_id = $1), \
                (SELECT count(*) FROM auth.user_sessions WHERE user_id = $1 AND revoked_at IS NULL)",
    )
    .bind(alice)
    .fetch_one(&h.owner)
    .await
    .unwrap();
    assert_eq!((remaining, active), (0, 0));

    // The last admin cannot be removed; another tenant's member is not found.
    assert_eq!(
        h.call(
            "DELETE",
            &format!("/v1/admin/members/{admin}"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let (_, other_admin, _) = h.tenant_with_admin().await;
    assert_eq!(
        h.call(
            "DELETE",
            &format!("/v1/admin/members/{other_admin}"),
            Some(&session),
            None
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );

    let mut actions = h.audit_actions(tenant).await;
    actions.sort();
    assert_eq!(
        actions,
        [
            "member_add",
            "member_remove",
            "member_role_update",
            "member_sessions_revoke"
        ]
    );
}
