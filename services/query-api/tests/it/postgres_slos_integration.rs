use query_api::slos::list_slos;
use uuid::Uuid;

#[tokio::test]
async fn list_slos_returns_seeded_dev_slo() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let slos = list_slos(&pool, tenant).await.unwrap();

    assert!(slos.iter().any(|s| s.service_name == "checkout"));
    let checkout = slos.iter().find(|s| s.service_name == "checkout").unwrap();
    assert_eq!(checkout.environment, "prod");
    assert_eq!(checkout.sli_type, "availability");
    assert!((checkout.target - 0.999).abs() < f64::EPSILON);
}
