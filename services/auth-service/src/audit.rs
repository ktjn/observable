use sqlx::PgPool;
use uuid::Uuid;

pub struct AuditEntry {
    pub credential_hash: String,
    pub tenant_id: Option<Uuid>,
    pub outcome: &'static str,
    pub denial_reason: Option<&'static str>,
}

impl AuditEntry {
    pub fn allow(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
        }
    }

    pub fn deny_not_found(credential_hash: String) -> Self {
        Self {
            credential_hash,
            tenant_id: None,
            outcome: "deny",
            denial_reason: Some("not_found"),
        }
    }

    pub fn deny(credential_hash: String, tenant_id: Uuid, reason: &'static str) -> Self {
        Self {
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "deny",
            denial_reason: Some(reason),
        }
    }
}

pub async fn write(db: &PgPool, entry: &AuditEntry) {
    let result = sqlx::query(
        "INSERT INTO credential_audit_log \
         (action, outcome, credential_hash, tenant_id, denial_reason) \
         VALUES ('credential_validate', $1, $2, $3, $4)",
    )
    .bind(entry.outcome)
    .bind(&entry.credential_hash)
    .bind(entry.tenant_id)
    .bind(entry.denial_reason)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to write credential audit log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_entry_fields() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let entry = AuditEntry::allow("abc".to_string(), tenant);
        assert_eq!(entry.outcome, "allow");
        assert_eq!(entry.tenant_id, Some(tenant));
        assert!(entry.denial_reason.is_none());
    }

    #[test]
    fn deny_not_found_entry_fields() {
        let entry = AuditEntry::deny_not_found("abc".to_string());
        assert_eq!(entry.outcome, "deny");
        assert_eq!(entry.denial_reason, Some("not_found"));
        assert!(entry.tenant_id.is_none());
    }

    #[test]
    fn deny_with_reason_entry_fields() {
        let tenant = Uuid::new_v4();
        let entry = AuditEntry::deny("abc".to_string(), tenant, "revoked");
        assert_eq!(entry.outcome, "deny");
        assert_eq!(entry.denial_reason, Some("revoked"));
        assert_eq!(entry.tenant_id, Some(tenant));
    }
}
