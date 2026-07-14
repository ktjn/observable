pub mod audit;
pub mod observability;
pub mod oidc;
pub mod session;
pub mod validate;

use anyhow::{Result, anyhow, bail};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Resolve the session-signing secret from an optional `SESSION_SECRET` env value.
///
/// Fails closed: outside dev mode, a missing or empty secret is an error rather than a
/// fallback to a default value, so a production install can never start with a guessable
/// session-signing secret.
pub fn resolve_session_secret(env_value: Option<String>, dev_mode: bool) -> Result<String> {
    match env_value {
        Some(secret) if !secret.is_empty() => Ok(secret),
        _ if dev_mode => Ok("dev-session-secret-change-in-prod!!".to_string()),
        _ => Err(anyhow!(
            "SESSION_SECRET must be set (OBSERVABLE_ENV is not \"dev\"); refusing to start with \
             a default session-signing secret"
        )),
    }
}

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

#[cfg(test)]
mod tests {
    use super::resolve_session_secret;

    #[test]
    fn dev_mode_falls_back_to_dev_default_when_unset() {
        let secret = resolve_session_secret(None, true).unwrap();
        assert_eq!(secret, "dev-session-secret-change-in-prod!!");
    }

    #[test]
    fn dev_mode_falls_back_to_dev_default_when_empty() {
        let secret = resolve_session_secret(Some(String::new()), true).unwrap();
        assert_eq!(secret, "dev-session-secret-change-in-prod!!");
    }

    #[test]
    fn non_dev_mode_uses_explicit_secret_when_set() {
        let secret = resolve_session_secret(Some("real-secret".to_string()), false).unwrap();
        assert_eq!(secret, "real-secret");
    }

    #[test]
    fn non_dev_mode_fails_closed_when_unset() {
        let err = resolve_session_secret(None, false).unwrap_err();
        assert!(err.to_string().contains("SESSION_SECRET"));
    }

    #[test]
    fn non_dev_mode_fails_closed_when_empty() {
        let err = resolve_session_secret(Some(String::new()), false).unwrap_err();
        assert!(err.to_string().contains("SESSION_SECRET"));
    }
}
