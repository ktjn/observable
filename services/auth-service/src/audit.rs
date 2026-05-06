use sqlx::PgPool;
use uuid::Uuid;

pub struct AuditEntry {
    pub action: &'static str,
    pub credential_hash: String,
    pub tenant_id: Option<Uuid>,
    pub outcome: &'static str,
    pub denial_reason: Option<&'static str>,
    pub auth_method: Option<&'static str>,
}

impl AuditEntry {
    pub fn allow(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("api_key"),
        }
    }

    pub fn deny_not_found(credential_hash: String) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: None,
            outcome: "deny",
            denial_reason: Some("not_found"),
            auth_method: Some("api_key"),
        }
    }

    pub fn deny(credential_hash: String, tenant_id: Uuid, reason: &'static str) -> Self {
        Self {
            action: "credential_validate",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "deny",
            denial_reason: Some(reason),
            auth_method: Some("api_key"),
        }
    }

    pub fn login(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "login",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("oidc_session"),
        }
    }

    pub fn logout(credential_hash: String, tenant_id: Uuid) -> Self {
        Self {
            action: "logout",
            credential_hash,
            tenant_id: Some(tenant_id),
            outcome: "allow",
            denial_reason: None,
            auth_method: Some("oidc_session"),
        }
    }
}

pub async fn write(db: &PgPool, entry: &AuditEntry) {
    let result = sqlx::query(
        "INSERT INTO credential_audit_log \
         (action, outcome, credential_hash, tenant_id, denial_reason, auth_method) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(entry.action)
    .bind(entry.outcome)
    .bind(&entry.credential_hash)
    .bind(entry.tenant_id)
    .bind(entry.denial_reason)
    .bind(entry.auth_method)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to write audit log");
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
        assert_eq!(entry.auth_method, Some("api_key"));
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

    #[test]
    fn login_entry_has_oidc_session_auth_method() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let entry = AuditEntry::login("user-jwt-hash".to_string(), tenant);
        assert_eq!(entry.action, "login");
        assert_eq!(entry.auth_method, Some("oidc_session"));
        assert_eq!(entry.outcome, "allow");
    }
}
