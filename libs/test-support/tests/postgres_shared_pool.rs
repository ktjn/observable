use sqlx::Row;

#[tokio::test]
async fn two_calls_share_a_container_but_get_isolated_databases() {
    let pool_a = test_support::postgres::shared_pool().await;
    let pool_b = test_support::postgres::shared_pool().await;

    // Migrations seed the dev tenant in every fresh database.
    let row_a = sqlx::query("SELECT count(*) AS n FROM tenants")
        .fetch_one(&pool_a)
        .await
        .expect("query against pool_a");
    assert!(row_a.get::<i64, _>("n") >= 1);

    // Insert into pool_a only; pool_b must not see it (separate databases).
    sqlx::query(
        "INSERT INTO tenants (id, name) VALUES ('11111111-1111-1111-1111-111111111111', 'only-in-a')",
    )
    .execute(&pool_a)
    .await
    .expect("insert into pool_a");

    let row_b = sqlx::query("SELECT count(*) AS n FROM tenants WHERE name = 'only-in-a'")
        .fetch_one(&pool_b)
        .await
        .expect("query against pool_b");
    assert_eq!(
        row_b.get::<i64, _>("n"),
        0,
        "pool_b must not see rows inserted into pool_a's database"
    );
}

#[tokio::test]
async fn isolated_pool_drops_its_database_with_the_guard() {
    let (pool, guard) = test_support::postgres::isolated_pool().await;
    let name: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .expect("current database");

    // Template clones carry the migrated schema and the search_path default.
    let tenants: i64 = sqlx::query_scalar("SELECT count(*) FROM tenants")
        .fetch_one(&pool)
        .await
        .expect("migrated tenants table");
    assert!(tenants >= 1);

    drop(guard);

    let observer = test_support::postgres::shared_pool().await;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)")
            .bind(&name)
            .fetch_one(&observer)
            .await
            .expect("pg_database lookup");
    assert!(!exists, "database {name} must be dropped with its guard");
}
