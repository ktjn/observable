pub mod oidc;
pub mod session;
pub mod validate;

use anyhow::{anyhow, bail, Result};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Look up an API key in the database and return `(tenant_id, role, environment)`.
/// Returns an error if the key is not found or has been revoked.
pub async fn lookup_api_key(pool: &PgPool, key: &str) -> Result<(Uuid, String, String)> {
    let hash = validate::sha256_hex(key);

    let row = sqlx::query(
        "SELECT tenant_id, key_hash, revoked_at, role, environment FROM api_keys WHERE key_hash = $1",
    )
    .bind(&hash)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        bail!("API key not found");
    };

    let environment: String = row.try_get("environment").unwrap_or_default();
    let entry = validate::ApiKeyEntry {
        tenant_id: row.try_get("tenant_id")?,
        key_hash: hash,
        revoked_at: row.try_get("revoked_at").unwrap_or(None),
        role: row.try_get("role")?,
    };

    let (tenant_id, role) =
        validate::validate_key_against_entry(key, &entry).map_err(|e| anyhow!("{e}"))?;
    Ok((tenant_id, role, environment))
}
