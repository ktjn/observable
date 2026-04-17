use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub struct ApiKeyEntry {
    pub tenant_id: Uuid,
    pub key_hash: String,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub fn sha256_hex(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn validate_key_against_entry(key: &str, entry: &ApiKeyEntry) -> Result<Uuid> {
    if entry.revoked_at.is_some() {
        bail!("API key has been revoked");
    }
    if sha256_hex(key) != entry.key_hash {
        bail!("Invalid API key");
    }
    Ok(entry.tenant_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key_returns_tenant_id() {
        let key = "dev-api-key-0000";
        let hash = sha256_hex(key);
        let entry = ApiKeyEntry {
            tenant_id: Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap(),
            key_hash: hash,
            revoked_at: None,
        };
        assert!(matches!(validate_key_against_entry(key, &entry), Ok(id) if id == entry.tenant_id));
    }

    #[test]
    fn wrong_key_is_rejected() {
        let entry = ApiKeyEntry {
            tenant_id: Uuid::new_v4(),
            key_hash: sha256_hex("correct-key"),
            revoked_at: None,
        };
        assert!(validate_key_against_entry("wrong-key", &entry).is_err());
    }

    #[test]
    fn revoked_key_is_rejected() {
        let key = "dev-api-key-0000";
        let entry = ApiKeyEntry {
            tenant_id: Uuid::new_v4(),
            key_hash: sha256_hex(key),
            revoked_at: Some(chrono::Utc::now()),
        };
        assert!(validate_key_against_entry(key, &entry).is_err());
    }
}
