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
