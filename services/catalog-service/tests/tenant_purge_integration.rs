// Tenant purge of the `catalog` schema as the scoped `observable_catalog` login: every tenant table is
// purged or explicitly retained, only the purged tenant's rows go, and a repeat purge is a no-op.

use catalog_service::tenant_purge::{self, STEPS};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use uuid::Uuid;

const RETAINED: &[&str] = &[];

async fn seed(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "WITH s AS (INSERT INTO catalog.services (tenant_id, business_name, operational_name, criticality) VALUES ($1, 'Pay', 'pay', 'critical') RETURNING id), j AS (INSERT INTO catalog.journeys (tenant_id, name, criticality) VALUES ($1, 'Checkout', 'critical') RETURNING id), p AS (INSERT INTO catalog.providers (tenant_id, name, kind) VALUES ($1, 'Acme', 'saas') RETURNING id), t AS (INSERT INTO catalog.teams (tenant_id, name) VALUES ($1, 'Team') RETURNING id), tm AS (INSERT INTO catalog.team_members (team_id, user_id) SELECT id, gen_random_uuid() FROM t), a AS (INSERT INTO catalog.service_aliases (service_id, tenant_id, alias) SELECT id, $1, 'payments' FROM s), o AS (INSERT INTO catalog.service_owners (service_id, tenant_id, role, principal_kind, principal_id) SELECT s.id, $1, 'business_owner', 'team', t.id FROM s, t), oc AS (INSERT INTO catalog.service_outcomes (service_id, tenant_id, statement, owner_kind, owner_id) SELECT s.id, $1, 'Customers pay', 'team', t.id FROM s, t), b AS (INSERT INTO catalog.service_bindings (service_id, tenant_id, service_name) SELECT id, $1, 'pay-api' FROM s), js AS (INSERT INTO catalog.journey_services (journey_id, service_id, tenant_id, step) SELECT j.id, s.id, $1, 0 FROM j, s), oa AS (INSERT INTO catalog.observability_assets (tenant_id, kind, external_id, owning_component, service_id) SELECT $1, 'dashboard', 'd1', 'observable-control', id FROM s), cc AS (INSERT INTO catalog.coverage_claims (service_id, tenant_id, item, claimed_by) SELECT id, $1, 'runbook', 'u' FROM s), cr AS (INSERT INTO catalog.coverage_runs (service_id, tenant_id, status, policy_source, window_minutes, assessed_at) SELECT id, $1, 'partial', 'default', 60, now() FROM s), ca AS (INSERT INTO catalog.coverage_assessments (service_id, tenant_id, item, demonstrated, assessed_at) SELECT id, $1, 'logs', false, now() FROM s), g AS (INSERT INTO catalog.coverage_gaps (tenant_id, item, service_id) SELECT $1, 'logs', id FROM s), pd AS (INSERT INTO catalog.provider_dependencies (tenant_id, provider_name, function, created_by, service_id) SELECT $1, 'Acme', 'card auth', 'u', id FROM s) INSERT INTO catalog.coverage_policies (tenant_id, criticality, items) VALUES ($1, 'critical', ARRAY['logs'])",
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
    assert_eq!(tenant_tables(&owner, "catalog").await, covered);
}

#[tokio::test]
async fn purge_removes_only_the_tenant_and_is_idempotent() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "catalog", kept).await;
    let gone_before = tenant_row_counts(&owner, "catalog", gone).await;

    let scoped = scoped_pool(&owner, "observable_catalog").await;
    let deleted = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(
        deleted.iter().all(|(_, n)| *n > 0),
        "seed covers every step: {deleted:?}"
    );

    for ((table, after), (_, before)) in tenant_row_counts(&owner, "catalog", gone)
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
        tenant_row_counts(&owner, "catalog", kept).await,
        kept_before
    );

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
                        assert_eq!(q.as_deref(), Some(concat!("component=", "catalog")));
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
    let kept_before = tenant_row_counts(&owner, "catalog", kept).await;
    let scoped = scoped_pool(&owner, "observable_catalog").await;

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
        tenant_row_counts(&owner, "catalog", gone)
            .await
            .iter()
            .all(|(_, n)| *n == 0)
    );
    assert_eq!(
        tenant_row_counts(&owner, "catalog", kept).await,
        kept_before
    );

    // Control unreachable: nothing is purged and nothing is acknowledged.
    let down =
        tenant_purge::PurgeFeed::new("http://127.0.0.1:1", "feed-token", reqwest::Client::new());
    assert!(tenant_purge::purge_pending(&scoped, &down).await.is_err());
    assert_eq!(
        tenant_row_counts(&owner, "catalog", kept).await,
        kept_before
    );
}
