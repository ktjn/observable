use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogRecord {
    pub tenant_id: Uuid,
    pub log_id: Uuid,
    pub timestamp_unix_nano: u64,
    pub observed_timestamp_unix_nano: u64,
    pub severity_number: i32,
    pub severity_text: String,
    pub body: serde_json::Value,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub attributes: HashMap<String, serde_json::Value>,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub service_name: String,
    pub environment: String,
    pub host_id: String,
    pub fingerprint: Option<u64>,
}

#[cfg(feature = "storage")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct LogRow {
    pub tenant_id: Uuid,
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

#[cfg(feature = "storage")]
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

#[cfg(feature = "storage")]
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
