use clickhouse::{Client, Row};
use domain::LogRecord;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Row, Serialize, Deserialize)]
pub struct LogRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub log_id: Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: String,
    pub resource_attributes: String,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
    pub fingerprint: Option<u64>,
}

impl From<LogRecord> for LogRow {
    fn from(l: LogRecord) -> Self {
        Self {
            tenant_id: l.tenant_id,
            log_id: l.log_id,
            timestamp_unix_nano: l.timestamp_unix_nano,
            observed_timestamp_unix_nano: l.observed_timestamp_unix_nano,
            severity_number: l.severity_number,
            severity_text: l.severity_text,
            body: serde_json::to_string(&l.body).unwrap_or_default(),
            trace_id: l.trace_id,
            span_id: l.span_id,
            attributes: serde_json::to_string(&l.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&l.resource_attributes).unwrap_or_default(),
            service_name: l.service_name,
            environment: l.environment,
            host_id: l.host_id,
            fingerprint: l.fingerprint,
        }
    }
}

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
}
