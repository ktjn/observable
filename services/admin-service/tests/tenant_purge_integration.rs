// Tenant purge of the `control` schema as the scoped `observable_control` login: every tenant table is
// purged or explicitly retained, only the purged tenant's rows go, and a repeat purge is a no-op.

use admin_service::tenant_purge::{self, STEPS};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use uuid::Uuid;

const RETAINED: &[&str] = tenant_purge::RETAINED;

async fn seed(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "INSERT INTO control.tenants (id, name) VALUES ($1, 'purge-target')",
        "INSERT INTO control.projects (tenant_id, name) VALUES ($1, 'p')",
        "INSERT INTO control.change_events (tenant_id, event_type, environment, title) VALUES ($1, 'other', 'prod', 't')",
        "INSERT INTO control.deployment_markers (tenant_id, service_name, environment, service_version, status) VALUES ($1, 's', 'prod', 'v1', 'success')",
        "INSERT INTO control.semantic_annotations (tenant_id, signal_type, field_name) VALUES ($1, 'metrics', 'f')",
        "INSERT INTO control.environment_classes (tenant_id, environment, class, updated_by) VALUES ($1, 'prod', 'production', 'u')",
        "WITH v AS (INSERT INTO control.saved_views (tenant_id, name, signal_kind, config) VALUES ($1, 'v', 'logs', '{}') RETURNING saved_view_id) INSERT INTO control.saved_view_grants (saved_view_id, user_id, relation) SELECT saved_view_id, gen_random_uuid(), 'owner' FROM v",
        "WITH d AS (INSERT INTO control.dashboards (tenant_id, name) VALUES ($1, 'd') RETURNING dashboard_id), p AS (INSERT INTO control.dashboard_panels (dashboard_id, title) SELECT dashboard_id, 'p' FROM d) INSERT INTO control.dashboard_grants (dashboard_id, user_id, relation) SELECT dashboard_id, gen_random_uuid(), 'owner' FROM d",
        "INSERT INTO control.query_audit_log (action, tenant_id, result_count) VALUES ('query', $1, 0)",
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
        .filter(|t| t != "tenants")
        .chain(RETAINED.iter().map(|t| t.to_string()))
        .collect();
    covered.sort();
    assert_eq!(tenant_tables(&owner, "control").await, covered);
}

#[tokio::test]
async fn purge_removes_only_the_tenant_and_is_idempotent() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "control", kept).await;
    let gone_before = tenant_row_counts(&owner, "control", gone).await;

    let scoped = scoped_pool(&owner, "observable_control").await;
    let deleted = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(
        deleted.iter().all(|(_, n)| *n > 0),
        "seed covers every step: {deleted:?}"
    );
    let tenant_rows: i64 = sqlx::query_scalar("SELECT count(*) FROM control.tenants WHERE id = $1")
        .bind(gone)
        .fetch_one(&owner)
        .await
        .unwrap();
    assert_eq!(tenant_rows, 0);
    let children: i64 = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM control.dashboard_panels) + (SELECT count(*) FROM control.saved_view_grants)",
    )
    .fetch_one(&owner)
    .await
    .unwrap();
    assert_eq!(children, 2, "only the kept tenant's panel and grant remain");

    for ((table, after), (_, before)) in tenant_row_counts(&owner, "control", gone)
        .await
        .iter()
        .zip(&gone_before)
    {
        let expected = if RETAINED.contains(&table.as_str()) {
            *before
        } else {
            0
        };
        assert_eq!(*after, expected, "{table}");
    }
    assert_eq!(
        tenant_row_counts(&owner, "control", kept).await,
        kept_before
    );

    let again = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(again.iter().all(|(_, n)| *n == 0), "{again:?}");
}

mod feed {
    use admin_service::{AdminServiceAppState, observability, tenant_purge};
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::{get, post},
    };
    use http_body_util::BodyExt;
    use serde_json::Value;
    use std::sync::Arc;
    use test_support::postgres::{isolated_pool, scoped_pool};
    use tower::ServiceExt;
    use uuid::Uuid;

    fn app(db: sqlx::PgPool) -> Router {
        Router::new()
            .route("/internal/tenant-purges", get(tenant_purge::handle_feed))
            .route(
                "/internal/tenant-purges/{tenant_id}/acks",
                post(tenant_purge::handle_ack),
            )
            .with_state(AdminServiceAppState {
                db,
                ch: clickhouse::Client::default(),
                auth_service_url: "http://127.0.0.1:1".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AdminServiceMetrics::new()),
                producer: None,
            })
    }

    async fn feed(app: &Router, component: &str) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::get(format!("/internal/tenant-purges?component={component}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&body).unwrap_or_default())
    }

    async fn ack(app: &Router, tenant: Uuid, component: &str) -> StatusCode {
        app.clone()
            .oneshot(
                Request::post(format!("/internal/tenant-purges/{tenant}/acks"))
                    .header("content-type", "application/json")
                    .body(Body::from(format!(r#"{{"component":"{component}"}}"#)))
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn components_pull_pending_purges_until_each_acknowledges() {
        let (owner, _db) = isolated_pool().await;
        let scoped = scoped_pool(&owner, "observable_control").await;
        let app = app(scoped.clone());
        let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
        tenant_purge::record_request(&scoped, first, "admin-1")
            .await
            .unwrap();
        tenant_purge::record_request(&scoped, second, "admin-2")
            .await
            .unwrap();
        tenant_purge::record_request(&scoped, first, "again")
            .await
            .unwrap();

        let (status, body) = feed(&app, "alerting").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["tenants"], serde_json::json!([first, second]));
        assert_eq!(
            feed(&app, "catalog").await.1["tenants"],
            serde_json::json!([]),
            "the catalog purges only after alerting stopped syncing the tenant"
        );
        assert_eq!(ack(&app, first, "alerting").await, StatusCode::NO_CONTENT);
        assert_eq!(ack(&app, second, "alerting").await, StatusCode::NO_CONTENT);
        assert_eq!(
            feed(&app, "catalog").await.1["tenants"],
            serde_json::json!([first, second])
        );

        assert_eq!(ack(&app, first, "catalog").await, StatusCode::NO_CONTENT);
        assert_eq!(
            ack(&app, first, "catalog").await,
            StatusCode::NO_CONTENT,
            "idempotent"
        );
        assert_eq!(
            feed(&app, "catalog").await.1["tenants"],
            serde_json::json!([second])
        );
        assert_eq!(
            feed(&app, "health").await.1["tenants"],
            serde_json::json!([first, second])
        );
        assert_eq!(
            tenant_purge::pending_components(&scoped, first)
                .await
                .unwrap(),
            ["health"]
        );

        let completed = || async {
            sqlx::query_scalar::<_, bool>(
                "SELECT completed_at IS NOT NULL FROM control.tenant_purges WHERE tenant_id = $1",
            )
            .bind(first)
            .fetch_one(&owner)
            .await
            .unwrap()
        };
        assert!(!completed().await);
        assert_eq!(ack(&app, first, "health").await, StatusCode::NO_CONTENT);
        assert!(completed().await);
        let requested_by: String = sqlx::query_scalar(
            "SELECT requested_by FROM control.tenant_purges WHERE tenant_id = $1",
        )
        .bind(first)
        .fetch_one(&owner)
        .await
        .unwrap();
        assert_eq!(
            requested_by, "admin-1",
            "a repeated request keeps the first"
        );
    }

    #[tokio::test]
    async fn unknown_components_and_requests_are_refused() {
        let (owner, _db) = isolated_pool().await;
        let app = app(scoped_pool(&owner, "observable_control").await);
        assert_eq!(
            feed(&app, "query").await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            ack(&app, Uuid::new_v4(), "catalog").await,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            ack(&app, Uuid::new_v4(), "query").await,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
