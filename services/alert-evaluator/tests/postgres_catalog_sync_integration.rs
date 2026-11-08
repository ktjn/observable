//! Alerting registers its rules and objectives in the catalog asset registry (phase 1 S6): the
//! sync worker reports each tenant's alert rules and SLOs to the catalog's internal sync route.

use alert_evaluator::catalog_sync::{CatalogClient, collect, sync_once};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn rule(db: &PgPool, tenant: Uuid, name: &str, service: Option<&str>) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO alert_rules (tenant_id, name, alert_type, severity, condition, service_name) \
         VALUES ($1, $2, 'threshold', 'warning', '{}'::jsonb, $3) RETURNING rule_id",
    )
    .bind(tenant)
    .bind(name)
    .bind(service)
    .fetch_one(db)
    .await
    .unwrap()
}

async fn slo(db: &PgPool, tenant: Uuid, service: &str, env: &str, description: &str) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO slo_definitions (tenant_id, service_name, environment, sli_type, target, \
         window_days, burn_rate_fast_threshold, burn_rate_slow_threshold, description) \
         VALUES ($1, $2, $3, 'availability', 0.99, 30, 14.4, 1.0, $4) RETURNING slo_id",
    )
    .bind(tenant)
    .bind(service)
    .bind(env)
    .bind(description)
    .fetch_one(db)
    .await
    .unwrap()
}

fn values<T: serde::Serialize>(v: &[T]) -> Vec<Value> {
    v.iter().map(|x| serde_json::to_value(x).unwrap()).collect()
}

#[tokio::test]
async fn rules_and_objectives_are_collected_per_tenant_and_kind() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
    let r = rule(&db, a, "Errors", Some(" payments-api ")).await;
    rule(&db, a, "Unattributed", None).await;
    rule(&db, a, "Blank", Some("  ")).await;
    let s = slo(&db, a, "payments-api", "prod", "").await;
    rule(&db, b, "Only unattributed", None).await;

    let all = collect(&db).await.unwrap();
    assert_eq!(
        values(&all[&a]["alert_rule"]),
        vec![
            json!({"external_id": r.to_string(), "service_name": "payments-api",
                    "environment": "", "name": "Errors"})
        ]
    );
    assert_eq!(
        values(&all[&a]["objective"]),
        vec![
            json!({"external_id": s.to_string(), "service_name": "payments-api",
                    "environment": "prod", "name": "payments-api availability 0.99"})
        ]
    );
    // A tenant with nothing attributable still gets both kinds, so stale assets are retired.
    assert!(all[&b]["alert_rule"].is_empty() && all[&b]["objective"].is_empty());
}

#[tokio::test]
async fn sync_posts_each_tenant_and_kind_with_the_internal_token() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let a = Uuid::new_v4();
    let s = slo(&db, a, "ledger", "prod", "Ledger availability").await;
    let tenants = collect(&db).await.unwrap().len();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/catalog/assets/sync"))
        .and(header("X-Internal-Token", "tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let client = CatalogClient::new(server.uri(), "tok", reqwest::Client::new());
    assert_eq!(sync_once(&db, &client).await.unwrap(), 0);

    let bodies: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), tenants * 2);
    let objective = bodies
        .iter()
        .find(|b| b["tenant_id"] == json!(a) && b["kind"] == "objective")
        .expect("objective sync");
    assert_eq!(objective["owning_component"], "observable-alerting");
    assert_eq!(objective["assets"][0]["external_id"], json!(s.to_string()));
    assert_eq!(objective["assets"][0]["name"], "Ledger availability");
    let rules = bodies
        .iter()
        .find(|b| b["tenant_id"] == json!(a) && b["kind"] == "alert_rule")
        .expect("rule sync");
    assert_eq!(rules["assets"], json!([]));
}

#[tokio::test]
async fn a_failing_catalog_is_counted_not_fatal() {
    let (db, _guard) = test_support::postgres::isolated_pool().await;
    let tenants = collect(&db).await.unwrap().len();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let client = CatalogClient::new(server.uri(), "tok", reqwest::Client::new());
    assert_eq!(sync_once(&db, &client).await.unwrap(), tenants * 2);
}
