use serde::Serialize;
use uuid::Uuid;

use crate::traces::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct QueryAuditEntry {
    pub action: &'static str,
    pub tenant_id: Uuid,
    pub result_count: i64,
}

pub async fn write(state: &AppState, entry: &QueryAuditEntry) {
    write_internal(
        &state.http_client,
        &state.admin_service_url,
        &state.internal_service_token,
        entry,
    )
    .await;
}

pub async fn write_internal(
    client: &reqwest::Client,
    admin_service_url: &str,
    internal_service_token: &str,
    entry: &QueryAuditEntry,
) {
    let url = format!(
        "{}/internal/query-audit",
        admin_service_url.trim_end_matches('/')
    );
    let res = client
        .post(&url)
        .header("x-internal-token", internal_service_token)
        .json(entry)
        .send()
        .await;

    match res {
        Ok(resp) if !resp.status().is_success() => {
            tracing::warn!(
                status = %resp.status(),
                "admin-service rejected query audit log"
            );
        }
        Err(e) => {
            tracing::warn!(error = %e, "failed to send query audit log to admin-service");
        }
        _ => {}
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
