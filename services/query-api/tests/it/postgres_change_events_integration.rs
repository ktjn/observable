use chrono::Utc;
use query_api::change_events::{ListChangeEventsParams, list_change_events};
use sqlx::PgPool;
use uuid::Uuid;

async fn seed_event(
    pool: &PgPool,
    tenant_id: Uuid,
    event_type: &str,
    service_name: Option<&str>,
    environment: &str,
    title: &str,
) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO change_events (tenant_id, event_type, service_name, environment, title) \
         VALUES ($1, $2, $3, $4, $5) RETURNING change_event_id",
    )
    .bind(tenant_id)
    .bind(event_type)
    .bind(service_name)
    .bind(environment)
    .bind(title)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn empty_params() -> ListChangeEventsParams {
    ListChangeEventsParams {
        service_name: None,
        environment: None,
        event_type: None,
        start_time: None,
        end_time: None,
        limit: None,
    }
}

#[tokio::test]
async fn list_returns_seeded_event() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();
    let id = seed_event(
        &pool,
        tenant,
        "feature_flag",
        Some("checkout"),
        "production",
        "Enabled new flow",
    )
    .await;

    let items = list_change_events(&pool, tenant, empty_params())
        .await
        .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].change_event_id, id);
    assert_eq!(items[0].event_type, "feature_flag");
    assert_eq!(items[0].service_name, Some("checkout".to_string()));
}

#[tokio::test]
async fn list_does_not_return_other_tenant_events() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    seed_event(
        &pool,
        tenant_a,
        "config_change",
        None,
        "staging",
        "Tenant A change",
    )
    .await;

    let tenant_b_items = list_change_events(&pool, tenant_b, empty_params())
        .await
        .unwrap();

    assert!(
        tenant_b_items.is_empty(),
        "tenant B must not see tenant A's change events"
    );
}

#[tokio::test]
async fn list_filters_by_service_name() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();
    seed_event(
        &pool,
        tenant,
        "migration",
        Some("checkout"),
        "production",
        "Checkout schema migration",
    )
    .await;
    seed_event(
        &pool,
        tenant,
        "migration",
        Some("billing"),
        "production",
        "Billing schema migration",
    )
    .await;

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams {
            service_name: Some("checkout".into()),
            ..empty_params()
        },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Checkout schema migration");
}

#[tokio::test]
async fn list_filters_by_event_type() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();
    seed_event(
        &pool,
        tenant,
        "incident",
        Some("checkout"),
        "production",
        "Incident annotation",
    )
    .await;
    seed_event(
        &pool,
        tenant,
        "config_change",
        Some("checkout"),
        "production",
        "Config change",
    )
    .await;

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams {
            event_type: Some("incident".into()),
            ..empty_params()
        },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Incident annotation");
}

#[tokio::test]
async fn list_respects_limit_cap() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();
    for i in 0..5 {
        seed_event(
            &pool,
            tenant,
            "other",
            None,
            "production",
            &format!("Event {i}"),
        )
        .await;
    }

    let items = list_change_events(
        &pool,
        tenant,
        ListChangeEventsParams {
            limit: Some(2),
            ..empty_params()
        },
    )
    .await
    .unwrap();

    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn list_orders_by_occurred_at_descending() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();
    let older = seed_event(&pool, tenant, "other", None, "production", "Older").await;
    sqlx::query(
        "UPDATE change_events SET occurred_at = NOW() - INTERVAL '1 hour' WHERE change_event_id = $1",
    )
    .bind(older)
    .execute(&pool)
    .await
    .unwrap();
    seed_event(&pool, tenant, "other", None, "production", "Newer").await;

    let items = list_change_events(&pool, tenant, empty_params())
        .await
        .unwrap();

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Newer");
    assert_eq!(items[1].title, "Older");
}

#[allow(unused)]
fn _use_utc() -> chrono::DateTime<Utc> {
    Utc::now()
}
