// Stub — full implementation in Task 10
use anyhow::Result;
use sqlx::PgPool;
pub async fn seed_dev_admin_role(_pool: &PgPool, _email: &str) -> Result<()> {
    Ok(())
}
