// dev_bootstrap.rs — only called when OBSERVABLE_ENV=dev.
// Ensures admin@dev.observable has a tenant_admin role on both the observable
// tenant, dev-tenant, and crypto-demo after Zitadel login.  The user row is created by the
// OIDC callback on first login; this pre-seeds role assignments so login ->
// tenant works immediately.
//
// This is a no-op if the user has not logged in yet (no users row).

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;

const OBSERVABLE_TENANT_ID: &str = "00000000-0000-0000-0000-000000000001";
const DEV_TENANT_ID: &str = "00000000-0000-0000-0000-000000000002";
const CRYPTO_DEMO_TENANT_ID: &str = "00000000-0000-0000-0000-000000000003";

pub async fn seed_dev_admin_role(pool: &PgPool, dev_admin_email: &str) -> Result<()> {
    let user_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
        .bind(dev_admin_email)
        .fetch_optional(pool)
        .await?;

    if let Some(uid) = user_id {
        for tenant_str in [OBSERVABLE_TENANT_ID, DEV_TENANT_ID, CRYPTO_DEMO_TENANT_ID] {
            let tenant_id = Uuid::parse_str(tenant_str)?;
            sqlx::query(
                r#"
                INSERT INTO user_tenant_roles (user_id, tenant_id, role)
                VALUES ($1, $2, 'tenant_admin')
                ON CONFLICT (user_id, tenant_id) DO NOTHING
                "#,
            )
            .bind(uid)
            .bind(tenant_id)
            .execute(pool)
            .await?;
        }
        tracing::info!(
            email = dev_admin_email,
            "dev admin role ensured on observable, dev-tenant, and crypto-demo"
        );
    } else {
        tracing::info!(
            email = dev_admin_email,
            "dev admin has not logged in yet; role will be seeded on first callback"
        );
    }

    Ok(())
}
