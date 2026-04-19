/// Pipeline definition schema — deserialized from the UI's JSON payload.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct PipelineDefinition {
    pub version: String,
    pub name: String,
    pub transport: TransportConfig,
    pub parser: ParserConfig,
    pub mapping: MappingConfig,
    pub output: OutputConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TransportConfig {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ParserConfig {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub params: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MappingConfig {
    pub resource_attributes: Option<HashMap<String, ValueSource>>,
    pub log_attributes: Option<HashMap<String, ValueSource>>,
    pub body: Option<ValueSource>,
    pub severity_text: Option<ValueSource>,
    pub severity_number: Option<ValueSource>,
    pub trace_id: Option<ValueSource>,
    pub span_id: Option<ValueSource>,
    pub time_field: Option<TimeFieldConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ValueSource {
    Field { field: String, #[serde(skip_serializing_if = "Option::is_none")] r#type: Option<String>, #[serde(skip_serializing_if = "Option::is_none")] map: Option<HashMap<String, String>> },
    Literal { literal: String },
    Env { env: String },
}

#[derive(Debug, Deserialize, Serialize)]
pub struct TimeFieldConfig {
    pub field: String,
    pub format: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OutputConfig {
    pub endpoint: String,
    pub protocol: String,
    pub headers: Option<HashMap<String, String>>,
    pub batch_size: Option<usize>,
    pub flush_interval_ms: Option<u64>,
}
