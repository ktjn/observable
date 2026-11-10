// Tenant purge of the `auth` schema as the scoped `observable_auth` login: every tenant table is
// purged or explicitly retained, only the purged tenant's rows go, and a repeat purge is a no-op.

use auth_service::tenant_purge::{self, STEPS};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use uuid::Uuid;

const RETAINED: &[&str] = tenant_purge::RETAINED;

async fn seed(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "WITH u AS (INSERT INTO auth.users (idp_subject, email) VALUES ($1::text, $1::text) RETURNING id), r AS (INSERT INTO auth.user_tenant_roles (user_id, tenant_id, role) SELECT id, $1, 'tenant_admin' FROM u), s AS (INSERT INTO auth.user_sessions (user_id, tenant_id, environment, expires_at) SELECT id, $1, 'prod', now() + interval '1 hour' FROM u) INSERT INTO auth.authz_elevations (tenant_id, user_id, resource_type, max_classification, justification, expires_at) SELECT $1, id, 'catalog.service', 'restricted', 'incident', now() + interval '1 hour' FROM u",
        "INSERT INTO auth.api_keys (tenant_id, key_hash, name) VALUES ($1, md5($1::text), 'k')",
        "INSERT INTO auth.health_worker_credentials (token_hash_sha256, tenant_id, expires_at) VALUES (encode(sha256($1::text::bytea), 'hex'), $1, now() + interval '1 day')",
        "INSERT INTO auth.authz_purpose_rules (tenant_id, resource_type, action, min_classification) VALUES ($1, 'catalog.service', 'read', 'restricted')",
        "INSERT INTO auth.authz_role_grants (tenant_id, role, resource_type, action, max_classification) VALUES ($1, 'viewer', 'catalog.service', 'read', 'internal')",
        "INSERT INTO auth.credential_audit_log (action, outcome, credential_hash, tenant_id) VALUES ('seed', 'allow', 'x', $1)",
        "INSERT INTO auth.access_decision_log (decision, tenant_id, principal_kind) VALUES ('permit', $1, 'user')",
    ] {
        sqlx::query(sql)
            .bind(tenant)
            .execute(owner)
            .await
            .expect(sql);
    }
}

#[tokio::test]
async fn every_tenant_table_is_purged_or_retained() {
    let (owner, _db) = isolated_pool().await;
    let mut covered: Vec<String> = STEPS
        .iter()
        .map(|(t, _)| t.to_string())
        .chain(RETAINED.iter().map(|t| t.to_string()))
        .collect();
    covered.sort();
    assert_eq!(tenant_tables(&owner, "auth").await, covered);
}

#[tokio::test]
async fn purge_removes_only_the_tenant_and_is_idempotent() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "auth", kept).await;
    let gone_before = tenant_row_counts(&owner, "auth", gone).await;

    let scoped = scoped_pool(&owner, "observable_auth").await;
    let deleted = tenant_purge::purge_tenant(&scoped, gone, "actor-1")
        .await
        .unwrap();
    assert!(
        deleted.iter().all(|(_, n)| *n > 0),
        "seed covers every step: {deleted:?}"
    );

    for ((table, after), (_, before)) in tenant_row_counts(&owner, "auth", gone)
        .await
        .iter()
        .zip(&gone_before)
    {
        // The purge adds one tenant_purge audit row per run; other retained rows stay.
        let expected = match table.as_str() {
            "credential_audit_log" => *before + 1,
            t if RETAINED.contains(&t) => *before,
            _ => 0,
        };
        assert_eq!(*after, expected, "{table}");
    }
    assert_eq!(tenant_row_counts(&owner, "auth", kept).await, kept_before);

    let again = tenant_purge::purge_tenant(&scoped, gone, "actor-1")
        .await
        .unwrap();
    assert!(again.iter().all(|(_, n)| *n == 0), "{again:?}");
    let audits: Vec<String> = sqlx::query_scalar(
        "SELECT credential_hash FROM auth.credential_audit_log \
         WHERE tenant_id = $1 AND action = 'tenant_purge' AND outcome = 'allow'",
    )
    .bind(gone)
    .fetch_all(&owner)
    .await
    .unwrap();
    assert_eq!(audits, ["actor-1", "actor-1"]);
}

#[tokio::test]
async fn internal_purge_route_requires_the_internal_token_and_an_actor() {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    const TOKEN: &str = "integration-internal-token";
    let (owner, _db) = isolated_pool().await;
    let tenant = Uuid::new_v4();
    seed(&owner, tenant).await;
    let app = auth_service::authz::router(auth_service::authz::AuthzState {
        db: scoped_pool(&owner, "observable_auth").await,
        internal_token: std::sync::Arc::new(TOKEN.into()),
    });
    let call = |token: Option<&'static str>, body: &'static str| {
        let mut req = Request::post(format!("/internal/tenants/{tenant}/purge"))
            .header("content-type", "application/json");
        if let Some(token) = token {
            req = req.header("x-internal-token", token);
        }
        app.clone().oneshot(req.body(Body::from(body)).unwrap())
    };
    assert_eq!(
        call(None, r#"{"actor":"a"}"#).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(Some("wrong"), r#"{"actor":"a"}"#)
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(Some(TOKEN), r#"{"actor":" "}"#)
            .await
            .unwrap()
            .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let api_keys = || async {
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM auth.api_keys WHERE tenant_id = $1")
            .bind(tenant)
            .fetch_one(&owner)
            .await
            .unwrap()
    };
    assert_eq!(api_keys().await, 1, "refused calls delete nothing");
    assert_eq!(
        call(Some(TOKEN), r#"{"actor":"a"}"#)
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    assert_eq!(api_keys().await, 0);
}
