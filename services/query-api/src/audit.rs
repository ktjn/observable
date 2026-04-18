use sqlx::PgPool;
use uuid::Uuid;

pub struct QueryAuditEntry {
    pub action: &'static str,
    pub tenant_id: Uuid,
    pub result_count: i64,
}

pub async fn write(db: &PgPool, entry: &QueryAuditEntry) {
    let result = sqlx::query(
        "INSERT INTO query_audit_log (action, tenant_id, result_count) \
         VALUES ($1, $2, $3)",
    )
    .bind(entry.action)
    .bind(entry.tenant_id)
    .bind(entry.result_count)
    .execute(db)
    .await;

    if let Err(e) = result {
        tracing::warn!(error = %e, "failed to write query audit log");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_get_entry_fields() {
        let tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let entry = QueryAuditEntry {
            action: "trace_get",
            tenant_id: tenant,
            result_count: 3,
        };
        assert_eq!(entry.action, "trace_get");
        assert_eq!(entry.tenant_id, tenant);
        assert_eq!(entry.result_count, 3);
    }

    #[test]
    fn log_search_entry_fields() {
        let tenant = Uuid::new_v4();
        let entry = QueryAuditEntry {
            action: "log_search",
            tenant_id: tenant,
            result_count: 0,
        };
        assert_eq!(entry.action, "log_search");
        assert_eq!(entry.result_count, 0);
    }

    #[test]
    fn metric_points_get_entry_fields() {
        let tenant = Uuid::new_v4();
        let entry = QueryAuditEntry {
            action: "metric_points_get",
            tenant_id: tenant,
            result_count: 100,
        };
        assert_eq!(entry.action, "metric_points_get");
        assert_eq!(entry.result_count, 100);
    }
}
