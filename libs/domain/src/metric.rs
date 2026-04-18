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

#[cfg(feature = "storage")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct MetricSeriesRow {
    pub tenant_id: Uuid,
    pub metric_series_id: Uuid,
    pub metric_name: String,
    pub description: String,
    pub unit: String,
    pub metric_type: String,
    pub is_monotonic: Option<u8>,
    pub aggregation_temporality: Option<String>,
    pub attributes: String,
    pub resource_attributes: String,
    pub service_name: String,
    pub environment: String,
}

#[cfg(feature = "storage")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct MetricPointRow {
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

#[cfg(feature = "storage")]
impl From<MetricSeries> for MetricSeriesRow {
    fn from(s: MetricSeries) -> Self {
        Self {
            tenant_id: s.tenant_id,
            metric_series_id: s.metric_series_id,
            metric_name: s.metric_name,
            description: s.description,
            unit: s.unit,
            metric_type: match s.metric_type {
                MetricType::ExponentialHistogram => "exponential_histogram".to_string(),
                _ => format!("{:?}", s.metric_type).to_lowercase(),
            },
            is_monotonic: s.is_monotonic.map(|m| if m { 1 } else { 0 }),
            aggregation_temporality: s
                .aggregation_temporality
                .map(|t| format!("{:?}", t).to_lowercase()),
            attributes: serde_json::to_string(&s.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&s.resource_attributes).unwrap_or_default(),
            service_name: s.service_name,
            environment: s.environment,
        }
    }
}

#[cfg(feature = "storage")]
impl From<MetricSeriesRow> for MetricSeries {
    fn from(row: MetricSeriesRow) -> Self {
        let metric_type = match row.metric_type.to_lowercase().as_str() {
            "gauge" => MetricType::Gauge,
            "sum" => MetricType::Sum,
            "histogram" => MetricType::Histogram,
            "exponential_histogram" | "exponentialhistogram" => MetricType::ExponentialHistogram,
            "summary" => MetricType::Summary,
            _ => MetricType::Gauge,
        };
        let aggregation_temporality =
            row.aggregation_temporality
                .and_then(|t| match t.to_lowercase().as_str() {
                    "delta" => Some(AggregationTemporality::Delta),
                    "cumulative" => Some(AggregationTemporality::Cumulative),
                    _ => None,
                });
        Self {
            tenant_id: row.tenant_id,
            metric_series_id: row.metric_series_id,
            metric_name: row.metric_name,
            description: row.description,
            unit: row.unit,
            metric_type,
            is_monotonic: row.is_monotonic.map(|m| m != 0),
            aggregation_temporality,
            attributes: serde_json::from_str(&row.attributes).unwrap_or_default(),
            resource_attributes: serde_json::from_str(&row.resource_attributes).unwrap_or_default(),
            service_name: row.service_name,
            environment: row.environment,
        }
    }
}

#[cfg(feature = "storage")]
impl From<MetricPoint> for MetricPointRow {
    fn from(p: MetricPoint) -> Self {
        Self {
            tenant_id: p.tenant_id,
            metric_series_id: p.metric_series_id,
            metric_name: p.metric_name,
            service_name: p.service_name,
            time_unix_nano: p.time_unix_nano,
            start_time_unix_nano: p.start_time_unix_nano,
            value_double: p.value_double,
            value_int: p.value_int,
            histogram_count: p.histogram_count,
            histogram_sum: p.histogram_sum,
            histogram_bucket_counts: p.histogram_bucket_counts,
            histogram_explicit_bounds: p.histogram_explicit_bounds,
        }
    }
}

#[cfg(feature = "storage")]
impl From<MetricPointRow> for MetricPoint {
    fn from(row: MetricPointRow) -> Self {
        Self {
            tenant_id: row.tenant_id,
            metric_series_id: row.metric_series_id,
            metric_name: row.metric_name,
            service_name: row.service_name,
            time_unix_nano: row.time_unix_nano,
            start_time_unix_nano: row.start_time_unix_nano,
            value_double: row.value_double,
            value_int: row.value_int,
            histogram_count: row.histogram_count,
            histogram_sum: row.histogram_sum,
            histogram_bucket_counts: row.histogram_bucket_counts,
            histogram_explicit_bounds: row.histogram_explicit_bounds,
        }
    }
}
