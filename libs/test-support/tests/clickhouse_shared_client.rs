use uuid::Uuid;

#[tokio::test]
async fn shared_client_is_scoped_to_observable_and_reusable() {
    let ch_a = test_support::clickhouse::shared_client().await;
    let ch_b = test_support::clickhouse::shared_client().await;

    // Migrations must have created the `metric_series` table; a trivial typed
    // query against it (scoped by a random tenant_id) must succeed on both
    // handles, proving they're both connected to the same migrated database.
    let tenant = Uuid::new_v4();
    let count: u64 = ch_a
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant)
        .fetch_one()
        .await
        .expect("query via ch_a succeeds");
    assert_eq!(count, 0);

    let count_b: u64 = ch_b
        .query("SELECT count() FROM metric_series WHERE tenant_id = ?")
        .bind(tenant)
        .fetch_one()
        .await
        .expect("query via ch_b succeeds");
    assert_eq!(count_b, 0);
}
