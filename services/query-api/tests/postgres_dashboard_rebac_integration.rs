//! Integration tests for dashboard ReBAC enforcement.
//! All tests use Testcontainers Postgres with all migrations applied.

use query_api::dashboards::{
    CreateDashboardRequest, DashboardPanelRequest, UpdateDashboardRequest, create_dashboard,
    get_dashboard, list_dashboards, update_dashboard,
};
use sqlx::PgPool;
use std::path::Path;
use testcontainers::{ImageExt, runners::AsyncRunner};
use testcontainers_modules::postgres::Postgres;
use uuid::Uuid;

async fn apply_migrations(pool: &PgPool) {
    let migrations_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("migrations/postgres");

    let mut entries: Vec<_> = std::fs::read_dir(&migrations_dir)
        .expect("migrations/postgres must exist")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "sql"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let sql = std::fs::read_to_string(entry.path()).expect("readable migration");
        sqlx::raw_sql(&sql)
            .execute(pool)
            .await
            .expect("migration applied");
    }
}

async fn start_pool() -> (PgPool, testcontainers::ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("postgres container started");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPool::connect(&url).await.expect("pool connected");
    apply_migrations(&pool).await;
    (pool, container)
}

async fn insert_tenant(pool: &PgPool, tenant_id: Uuid) {
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(tenant_id)
        .bind(format!("tenant-{tenant_id}"))
        .execute(pool)
        .await
        .expect("tenant inserted");
}

async fn insert_user(pool: &PgPool, tenant_id: Uuid) -> Uuid {
    let user_id = Uuid::new_v4();
    sqlx::query("INSERT INTO users (id, idp_subject, email) VALUES ($1, $2, $3)")
        .bind(user_id)
        .bind(format!("sub-{user_id}"))
        .bind(format!("{user_id}@test.com"))
        .execute(pool)
        .await
        .expect("user inserted");
    sqlx::query(
        "INSERT INTO user_tenant_roles (user_id, tenant_id, role) VALUES ($1, $2, 'member')",
    )
    .bind(user_id)
    .bind(tenant_id)
    .execute(pool)
    .await
    .expect("role assigned");
    user_id
}

fn one_panel() -> Vec<DashboardPanelRequest> {
    vec![DashboardPanelRequest {
        title: "Test panel".into(),
        query_kind: Some("logs".into()),
        ..Default::default()
    }]
}

// ── Test 1: creator gets owner grant ────────────────────────────────────────

#[tokio::test]
async fn create_dashboard_assigns_owner_grant_to_creator() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "My dash".into(),
            panels: one_panel(),
        },
        Some(user),
    )
    .await
    .unwrap();

    let relation: Option<String> = sqlx::query_scalar(
        "SELECT relation FROM dashboard_grants WHERE dashboard_id = $1 AND user_id = $2",
    )
    .bind(dashboard.dashboard_id)
    .bind(user)
    .fetch_optional(&pool)
    .await
    .unwrap();

    assert_eq!(relation.as_deref(), Some("owner"));
}

// ── Test 2: public dashboard visible to all members ─────────────────────────

#[tokio::test]
async fn public_dashboard_visible_to_any_member() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Public".into(),
            panels: one_panel(),
        },
        Some(creator),
    )
    .await
    .unwrap();
    assert_eq!(dashboard.visibility, "public");

    // other user can list and get the dashboard
    let listed = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        listed
            .iter()
            .any(|d| d.dashboard_id == dashboard.dashboard_id),
        "other user must see public dashboard in list"
    );

    let fetched = get_dashboard(&pool, tenant, dashboard.dashboard_id)
        .await
        .unwrap();
    assert!(
        fetched.is_some(),
        "get_dashboard must return public dashboard"
    );
}

// ── Test 3: private dashboard hidden from non-granted users ─────────────────

#[tokio::test]
async fn private_dashboard_hidden_from_non_granted_user() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
        },
        Some(creator),
    )
    .await
    .unwrap();

    // flip to private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // other user should NOT see it in list
    let listed = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        !listed
            .iter()
            .any(|d| d.dashboard_id == dashboard.dashboard_id),
        "other user must not see private dashboard in list"
    );

    // Also verify get_dashboard returns the dashboard regardless of user (library fn doesn't check grants;
    // the HTTP handler does — this confirms the data is still accessible via the library path)
    let fetched = get_dashboard(&pool, tenant, dashboard.dashboard_id)
        .await
        .unwrap();
    assert!(
        fetched.is_some(),
        "get_dashboard (library fn) always returns the row; HTTP handler enforces 403"
    );
}

// ── Test 4: private dashboard visible after explicit viewer grant ────────────

#[tokio::test]
async fn private_dashboard_visible_after_viewer_grant() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let viewer = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Shared".into(),
            panels: one_panel(),
        },
        Some(creator),
    )
    .await
    .unwrap();

    // flip to private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Shared".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // grant viewer access
    sqlx::query(
        "INSERT INTO dashboard_grants (dashboard_id, user_id, relation) VALUES ($1, $2, 'viewer')",
    )
    .bind(dashboard.dashboard_id)
    .bind(viewer)
    .execute(&pool)
    .await
    .unwrap();

    // viewer should now see it in list
    let listed = list_dashboards(&pool, tenant, Some(viewer)).await.unwrap();
    assert!(
        listed
            .iter()
            .any(|d| d.dashboard_id == dashboard.dashboard_id),
        "viewer must see private dashboard after explicit grant"
    );
}

// ── Test 5: flip back to public restores access ──────────────────────────────

#[tokio::test]
async fn flip_to_public_restores_member_access() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let creator = insert_user(&pool, tenant).await;
    let other = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Toggle".into(),
            panels: one_panel(),
        },
        Some(creator),
    )
    .await
    .unwrap();

    // flip private
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Toggle".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // confirm hidden
    let hidden = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        !hidden
            .iter()
            .any(|d| d.dashboard_id == dashboard.dashboard_id),
        "private dashboard must not appear in list for non-granted user"
    );

    // flip public
    update_dashboard(
        &pool,
        tenant,
        dashboard.dashboard_id,
        &UpdateDashboardRequest {
            name: "Toggle".into(),
            panels: one_panel(),
            visibility: Some("public".into()),
        },
    )
    .await
    .unwrap();

    // confirm visible again
    let visible = list_dashboards(&pool, tenant, Some(other)).await.unwrap();
    assert!(
        visible
            .iter()
            .any(|d| d.dashboard_id == dashboard.dashboard_id),
        "dashboard must be visible again after flip to public"
    );
}

// ── Test 6: revoke last owner grant returns error ────────────────────────────

#[tokio::test]
async fn revoke_last_owner_is_blocked_by_cte_guard() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let dashboard = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Solo owner".into(),
            panels: one_panel(),
        },
        Some(user),
    )
    .await
    .unwrap();

    // Attempt to delete the only owner using the same CTE guard the handler uses.
    // The guard prevents deletion when no other owners remain.
    let result = sqlx::query(
        "WITH guard AS ( \
           SELECT COUNT(*) AS remaining \
           FROM dashboard_grants \
           WHERE dashboard_id = $1 AND relation = 'owner' AND user_id != $2 \
         ) \
         DELETE FROM dashboard_grants \
         WHERE dashboard_id = $1 AND user_id = $2 \
           AND (SELECT remaining FROM guard) > 0",
    )
    .bind(dashboard.dashboard_id)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(
        result.rows_affected(),
        0,
        "CTE guard must prevent deletion of the last owner"
    );

    // Confirm the grant row still exists (was not deleted)
    let still_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM dashboard_grants WHERE dashboard_id = $1 AND user_id = $2)",
    )
    .bind(dashboard.dashboard_id)
    .bind(user)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        still_exists,
        "owner grant must still exist after blocked revocation"
    );
}

// ── Test 7: API-key path (user_id = None) sees all dashboards ───────────────

#[tokio::test]
async fn api_key_caller_sees_all_dashboards_regardless_of_visibility() {
    let (pool, _container) = start_pool().await;
    let tenant = Uuid::new_v4();
    insert_tenant(&pool, tenant).await;
    let user = insert_user(&pool, tenant).await;

    let public_dash = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Public".into(),
            panels: one_panel(),
        },
        Some(user),
    )
    .await
    .unwrap();

    let private_dash = create_dashboard(
        &pool,
        tenant,
        &CreateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
        },
        Some(user),
    )
    .await
    .unwrap();

    update_dashboard(
        &pool,
        tenant,
        private_dash.dashboard_id,
        &UpdateDashboardRequest {
            name: "Private".into(),
            panels: one_panel(),
            visibility: Some("private".into()),
        },
    )
    .await
    .unwrap();

    // API-key path: user_id = None
    let all = list_dashboards(&pool, tenant, None).await.unwrap();
    let ids: Vec<_> = all.iter().map(|d| d.dashboard_id).collect();
    assert!(ids.contains(&public_dash.dashboard_id));
    assert!(ids.contains(&private_dash.dashboard_id));
}
