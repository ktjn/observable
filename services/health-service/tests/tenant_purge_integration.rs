// Tenant purge of the `health` schema as the scoped `observable_health` login: every tenant table is
// purged or explicitly retained, only the purged tenant's rows go, and a repeat purge is a no-op.

use health_service::tenant_purge::{self, STEPS};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use uuid::Uuid;

const RETAINED: &[&str] = &[];

async fn seed(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "INSERT INTO health.rules (rule_id, revision, tenant_id, freshness_seconds, lookback_seconds, conditions) VALUES ('tenant-' || $1::text, 1, $1, 60, 300, '[{}]')",
        "INSERT INTO health.assessments (tenant_id, service_id, environment_class, rule_id, rule_revision, scope_key, identities, state, conditions, freshness_seconds, evaluated_at) VALUES ($1, gen_random_uuid(), 'production', 'platform-default', 1, 'all', '[]', 'unknown', '[]', 60, now())",
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
    assert_eq!(tenant_tables(&owner, "health").await, covered);
}

#[tokio::test]
async fn purge_removes_only_the_tenant_and_is_idempotent() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "health", kept).await;
    let gone_before = tenant_row_counts(&owner, "health", gone).await;

    let scoped = scoped_pool(&owner, "observable_health").await;
    let deleted = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(
        deleted.iter().all(|(_, n)| *n > 0),
        "seed covers every step: {deleted:?}"
    );
    let platform: i64 =
        sqlx::query_scalar("SELECT count(*) FROM health.rules WHERE tenant_id IS NULL")
            .fetch_one(&owner)
            .await
            .unwrap();
    assert!(
        platform > 0,
        "platform-default rules survive a tenant purge"
    );

    for ((table, after), (_, before)) in tenant_row_counts(&owner, "health", gone)
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
    assert_eq!(tenant_row_counts(&owner, "health", kept).await, kept_before);

    let again = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(again.iter().all(|(_, n)| *n == 0), "{again:?}");
}

/// Stand-in for observable-control's purge feed: serves `pending` and records acks.
async fn fake_control(
    pending: Vec<Uuid>,
) -> (
    String,
    std::sync::Arc<std::sync::Mutex<Vec<(Uuid, String)>>>,
) {
    use axum::{
        Json, Router,
        extract::Path,
        http::HeaderMap,
        routing::{get, post},
    };
    let acks = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = acks.clone();
    let app = Router::new()
        .route(
            "/internal/tenant-purges",
            get(
                move |headers: HeaderMap, axum::extract::RawQuery(q): axum::extract::RawQuery| {
                    let pending = pending.clone();
                    async move {
                        assert_eq!(headers["x-internal-token"], "feed-token");
                        assert_eq!(q.as_deref(), Some(concat!("component=", "health")));
                        Json(serde_json::json!({ "tenants": pending }))
                    }
                },
            ),
        )
        .route(
            "/internal/tenant-purges/{tenant}/acks",
            post(
                move |Path(tenant): Path<Uuid>, Json(body): Json<serde_json::Value>| {
                    let recorded = recorded.clone();
                    async move {
                        recorded
                            .lock()
                            .unwrap()
                            .push((tenant, body["component"].as_str().unwrap().to_owned()));
                        axum::http::StatusCode::NO_CONTENT
                    }
                },
            ),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (url, acks)
}

#[tokio::test]
async fn the_worker_purges_pending_tenants_from_the_control_feed_then_acknowledges() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "health", kept).await;
    let scoped = scoped_pool(&owner, "observable_health").await;

    let (url, acks) = fake_control(vec![gone]).await;
    let feed = tenant_purge::PurgeFeed::new(&url, "feed-token", reqwest::Client::new());
    assert_eq!(
        tenant_purge::purge_pending(&scoped, &feed).await.unwrap(),
        [gone]
    );
    assert_eq!(
        *acks.lock().unwrap(),
        [(gone, tenant_purge::COMPONENT.to_owned())]
    );
    assert!(
        tenant_row_counts(&owner, "health", gone)
            .await
            .iter()
            .all(|(_, n)| *n == 0)
    );
    assert_eq!(tenant_row_counts(&owner, "health", kept).await, kept_before);

    // Control unreachable: nothing is purged and nothing is acknowledged.
    let down =
        tenant_purge::PurgeFeed::new("http://127.0.0.1:1", "feed-token", reqwest::Client::new());
    assert!(tenant_purge::purge_pending(&scoped, &down).await.is_err());
    assert_eq!(tenant_row_counts(&owner, "health", kept).await, kept_before);
}
