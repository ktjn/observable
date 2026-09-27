use domain::LogRecord;

use crate::generated::logs::LogsLogRowV1;

pub type LogRow = LogsLogRowV1;

impl From<LogRecord> for LogRow {
    fn from(l: LogRecord) -> Self {
        Self {
            tenant_id: l.tenant_id,
            log_id: l.log_id,
            timestamp_unix_nano: l.timestamp_unix_nano,
            observed_timestamp_unix_nano: l.observed_timestamp_unix_nano,
            severity_number: l.severity_number,
            severity_text: l.severity_text,
            body: l.body.to_string(),
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

impl From<LogRow> for LogRecord {
    fn from(row: LogRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            log_id: row.log_id,
            timestamp_unix_nano: row.timestamp_unix_nano,
            observed_timestamp_unix_nano: row.observed_timestamp_unix_nano,
            severity_number: row.severity_number,
            severity_text: row.severity_text,
            body: serde_json::from_str(&row.body).unwrap_or_default(),
            trace_id: row.trace_id,
            span_id: row.span_id,
            attributes: serde_json::from_str(&row.attributes).unwrap_or_default(),
            resource_attributes: serde_json::from_str(&row.resource_attributes).unwrap_or_default(),
            service_name: row.service_name,
            environment: row.environment,
            host_id: row.host_id,
            fingerprint: row.fingerprint,
        }
    }
}
