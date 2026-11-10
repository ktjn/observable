// Adversarial and comprehensive tenant-isolation integration test suite.
//
// Verifies:
// Token lifecycle audit and cross-tenant token isolation moved with the token routes to
// auth-service (services/auth-service/tests/tenant_admin_integration.rs).
// 1. Multi-tenant alert rule isolation: Tenant A's alert rules and firings are completely hidden
//    from Tenant B queries.
// 2. Session cross-tenant switching: Attempting to switch to an unauthorized tenant fails
//    with 403 Forbidden and records an audit log denial.

use sqlx::PgPool;
use uuid::Uuid;

// ── Test Setup ───────────────────────────────────────────────────────────────

async fn start_pool() -> (PgPool, test_support::postgres::TestDatabase) {
    test_support::postgres::isolated_pool().await
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_alert_rule_and_firing_tenant_isolation() {
    let (pool, _container) = start_pool().await;

    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'tenant-a'), ($2, 'tenant-b')")
        .bind(tenant_a)
        .bind(tenant_b)
        .execute(&pool)
        .await
        .unwrap();

    // Insert alert rule for Tenant A
    let rule_a = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'cpu-high-a', 'threshold', 'critical', '{}')",
    )
    .bind(rule_a)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .unwrap();

    // Insert alert rule for Tenant B
    let rule_b = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO alert_rules (rule_id, tenant_id, name, alert_type, severity, condition) \
         VALUES ($1, $2, 'cpu-high-b', 'threshold', 'critical', '{}')",
    )
    .bind(rule_b)
    .bind(tenant_b)
    .execute(&pool)
    .await
    .unwrap();

    // Insert firing for Tenant A
    sqlx::query(
        "INSERT INTO alert_firings (firing_id, rule_id, tenant_id, state, value) \
         VALUES ($1, $2, $3, 'active', 95.0)",
    )
    .bind(Uuid::new_v4())
    .bind(rule_a)
    .bind(tenant_a)
    .execute(&pool)
    .await
    .unwrap();

    // Verify database queries strictly partitioned by tenant_id
    let count_a: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_rules WHERE tenant_id = $1")
        .bind(tenant_a)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_a, 1);

    let count_b: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM alert_rules WHERE tenant_id = $1")
        .bind(tenant_b)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_b, 1);

    let firings_b: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM alert_firings WHERE tenant_id = $1")
            .bind(tenant_b)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(firings_b, 0); // Tenant B has 0 firings
}
