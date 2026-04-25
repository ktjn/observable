use axum::http::StatusCode;
use serde_json::Value;
use uuid::Uuid;

pub fn extract_string_attr(attrs: &Value, key: &str) -> Option<String> {
    attrs
        .as_array()?
        .iter()
        .find(|a| a.get("key").and_then(|k| k.as_str()) == Some(key))?
        .get("value")?
        .get("stringValue")?
        .as_str()
        .map(String::from)
}

pub fn parse_otlp_logs(
    body: &Value,
    tenant_id: Uuid,
) -> Result<Vec<domain::LogRecord>, StatusCode> {
    let resource_logs = body
        .get("resourceLogs")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_REQUEST)?;

    let mut logs = Vec::new();
    for rl in resource_logs {
        let resource_attrs = rl
            .get("resource")
            .and_then(|r| r.get("attributes"))
            .cloned()
            .unwrap_or_default();
        let service_name = extract_string_attr(&resource_attrs, "service.name").unwrap_or_default();
        for scope_logs in rl
            .get("scopeLogs")
            .and_then(|v| v.as_array())
            .unwrap_or(&vec![])
        {
            for lr in scope_logs
                .get("logRecords")
                .and_then(|v| v.as_array())
                .unwrap_or(&vec![])
            {
                logs.push(domain::LogRecord {
                    tenant_id,
                    log_id: Uuid::new_v4(),
                    timestamp_unix_nano: lr
                        .get("timeUnixNano")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(0),
                    observed_timestamp_unix_nano: 0,
                    severity_number: lr
                        .get("severityNumber")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32,
                    severity_text: lr
                        .get("severityText")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .into(),
                    body: lr.get("body").cloned().unwrap_or(Value::Null),
                    service_name: service_name.clone(),
                    ..Default::default()
                });
            }
        }
    }
    Ok(logs)
}
