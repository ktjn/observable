use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricSeries {
    pub tenant_id: Uuid,
    pub metric_series_id: Uuid,
    pub metric_name: String,
    pub description: String,
    pub unit: String,
    pub metric_type: MetricType,
    pub is_monotonic: Option<bool>,
    pub aggregation_temporality: Option<AggregationTemporality>,
    pub attributes: HashMap<String, String>,
    pub resource_attributes: HashMap<String, serde_json::Value>,
    pub service_name: String,
    pub environment: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    #[default]
    Gauge,
    Sum,
    Histogram,
    ExponentialHistogram,
    Summary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AggregationTemporality {
    Delta,
    Cumulative,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetricPoint {
    pub tenant_id: Uuid,
    pub metric_series_id: Uuid,
    pub metric_name: String,
    pub service_name: String,
    pub time_unix_nano: u64,
    pub start_time_unix_nano: Option<u64>,
    pub value_double: Option<f64>,
    pub value_int: Option<i64>,
    pub histogram_count: Option<u64>,
    pub histogram_sum: Option<f64>,
    pub histogram_bucket_counts: Option<Vec<u64>>,
    pub histogram_explicit_bounds: Option<Vec<f64>>,
}
