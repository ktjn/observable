// Tenant purge of the `alerting` schema as the scoped `observable_alerting` login: every tenant table is
// purged or explicitly retained, only the purged tenant's rows go, and a repeat purge is a no-op.

use alert_evaluator::tenant_purge::{self, STEPS};
use sqlx::PgPool;
use test_support::postgres::{isolated_pool, scoped_pool, tenant_row_counts, tenant_tables};
use uuid::Uuid;

const RETAINED: &[&str] = &[];

async fn seed(owner: &PgPool, tenant: Uuid) {
    for sql in [
        "WITH r AS (INSERT INTO alerting.alert_rules (tenant_id, name, alert_type, severity, condition) VALUES ($1, 'r', 'threshold', 'warning', '{}') RETURNING rule_id), f AS (INSERT INTO alerting.alert_firings (rule_id, tenant_id, state) SELECT rule_id, $1, 'active' FROM r RETURNING firing_id), c AS (INSERT INTO alerting.notification_channels (tenant_id, name, type, config) VALUES ($1, 'c', 'webhook', '{}') RETURNING channel_id) INSERT INTO alerting.notification_audit_log (tenant_id, firing_id, channel_id, trigger_state) SELECT $1, firing_id, channel_id, 'active' FROM f, c",
        "WITH i AS (INSERT INTO alerting.incidents (tenant_id, title, severity, status, dedup_key) VALUES ($1, 'i', 'warning', 'triggered', 'k') RETURNING incident_id) INSERT INTO alerting.incident_events (incident_id, event_type) SELECT incident_id, 'triggered' FROM i",
        "INSERT INTO alerting.slo_definitions (tenant_id, service_name, environment, sli_type, target, window_days, burn_rate_fast_threshold, burn_rate_slow_threshold) VALUES ($1, 'svc', 'prod', 'availability', 0.99, 30, 14.4, 6)",
        "INSERT INTO alerting.service_objectives (tenant_id, service_id, environment_class, sli_type, target, window_days) VALUES ($1, gen_random_uuid(), 'production', 'availability', 0.99, 30)",
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
    assert_eq!(tenant_tables(&owner, "alerting").await, covered);
}

#[tokio::test]
async fn purge_removes_only_the_tenant_and_is_idempotent() {
    let (owner, _db) = isolated_pool().await;
    let (gone, kept) = (Uuid::new_v4(), Uuid::new_v4());
    seed(&owner, gone).await;
    seed(&owner, kept).await;
    let kept_before = tenant_row_counts(&owner, "alerting", kept).await;
    let gone_before = tenant_row_counts(&owner, "alerting", gone).await;

    let scoped = scoped_pool(&owner, "observable_alerting").await;
    let deleted = tenant_purge::purge_tenant(&scoped, gone).await.unwrap();
    assert!(
        deleted.iter().all(|(_, n)| *n > 0),
        "seed covers every step: {deleted:?}"
    );

    for ((table, after), (_, before)) in tenant_row_counts(&owner, "alerting", gone)
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
        tenant_row_counts(&owner, "alerting", kept).await,
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
                        assert_eq!(q.as_deref(), Some(concat!("component=", "alerting")));
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
    let kept_before = tenant_row_counts(&owner, "alerting", kept).await;
    let scoped = scoped_pool(&owner, "observable_alerting").await;

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
        tenant_row_counts(&owner, "alerting", gone)
            .await
            .iter()
            .all(|(_, n)| *n == 0)
    );
    assert_eq!(
        tenant_row_counts(&owner, "alerting", kept).await,
        kept_before
    );

    // Control unreachable: nothing is purged and nothing is acknowledged.
    let down =
        tenant_purge::PurgeFeed::new("http://127.0.0.1:1", "feed-token", reqwest::Client::new());
    assert!(tenant_purge::purge_pending(&scoped, &down).await.is_err());
    assert_eq!(
        tenant_row_counts(&owner, "alerting", kept).await,
        kept_before
    );
}
