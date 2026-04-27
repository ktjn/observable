use clickhouse::Client;
use domain::{LogRecord, LogRow};

pub async fn insert_logs(ch: &Client, logs: Vec<LogRecord>) -> anyhow::Result<()> {
    let mut insert = ch.insert::<LogRow>("logs").await?;
    for log in logs {
        insert.write(&LogRow::from(log)).await?;
    }
    insert.end().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::LogRecord;
    use uuid::Uuid;

    fn make_log(tenant_id: Uuid) -> LogRecord {
        LogRecord {
            tenant_id,
            log_id: Uuid::new_v4(),
            timestamp_unix_nano: 1_700_000_000_000_000_000,
            observed_timestamp_unix_nano: 1_700_000_000_001_000_000,
            severity_number: 17,
            severity_text: "ERROR".into(),
            body: serde_json::json!({"msg": "request failed", "code": 503}),
            trace_id: Some("abc123".into()),
            span_id: Some("def456".into()),
            attributes: [("http.status_code".to_string(), serde_json::json!(503))]
                .into_iter()
                .collect(),
            resource_attributes: [("host.name".to_string(), serde_json::json!("web-1"))]
                .into_iter()
                .collect(),
            service_name: "checkout".into(),
            environment: "prod".into(),
            host_id: "web-1".into(),
            fingerprint: Some(0xdeadbeef),
        }
    }

    #[test]
    fn log_row_from_record_maps_severity() {
        let log = LogRecord {
            tenant_id: Uuid::new_v4(),
            log_id: Uuid::new_v4(),
            severity_number: 17,
            severity_text: "ERROR".into(),
            body: serde_json::json!("request failed"),
            ..Default::default()
        };
        let row = LogRow::from(log);
        assert_eq!(row.severity_number, 17);
    }

    #[test]
    fn log_row_preserves_tenant_id() {
        let tenant_id = Uuid::new_v4();
        let log = LogRecord {
            tenant_id,
            ..Default::default()
        };
        let row = LogRow::from(log);
        assert_eq!(row.tenant_id, tenant_id);
    }

    #[test]
    fn log_row_roundtrip_preserves_all_fields() {
        let tenant_id = Uuid::new_v4();
        let original = make_log(tenant_id);
        let row = LogRow::from(original.clone());
        let recovered = LogRecord::from(row);

        assert_eq!(recovered.tenant_id, original.tenant_id);
        assert_eq!(recovered.log_id, original.log_id);
        assert_eq!(recovered.timestamp_unix_nano, original.timestamp_unix_nano);
        assert_eq!(
            recovered.observed_timestamp_unix_nano,
            original.observed_timestamp_unix_nano
        );
        assert_eq!(recovered.severity_number, original.severity_number);
        assert_eq!(recovered.severity_text, original.severity_text);
        assert_eq!(recovered.body, original.body);
        assert_eq!(recovered.trace_id, original.trace_id);
        assert_eq!(recovered.span_id, original.span_id);
        assert_eq!(recovered.attributes, original.attributes);
        assert_eq!(recovered.resource_attributes, original.resource_attributes);
        assert_eq!(recovered.service_name, original.service_name);
        assert_eq!(recovered.environment, original.environment);
        assert_eq!(recovered.host_id, original.host_id);
        assert_eq!(recovered.fingerprint, original.fingerprint);
    }

    #[test]
    fn log_row_with_null_optional_fields_roundtrips() {
        let tenant_id = Uuid::new_v4();
        let original = LogRecord {
            tenant_id,
            log_id: Uuid::new_v4(),
            body: serde_json::json!("plain message"),
            trace_id: None,
            span_id: None,
            fingerprint: None,
            ..Default::default()
        };
        let row = LogRow::from(original.clone());
        assert!(row.trace_id.is_none());
        assert!(row.span_id.is_none());
        assert!(row.fingerprint.is_none());

        let recovered = LogRecord::from(row);
        assert_eq!(recovered.trace_id, None);
        assert_eq!(recovered.span_id, None);
        assert_eq!(recovered.fingerprint, None);
    }
}
