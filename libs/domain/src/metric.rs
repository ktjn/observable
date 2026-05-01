use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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

pub fn deterministic_metric_series_id(series: &MetricSeries) -> Uuid {
    let key = serde_json::json!({
        "tenant_id": series.tenant_id,
        "metric_name": series.metric_name,
        "metric_type": series.metric_type,
        "is_monotonic": series.is_monotonic,
        "aggregation_temporality": series.aggregation_temporality,
        "attributes": BTreeMap::from_iter(series.attributes.iter()),
        "resource_attributes": BTreeMap::from_iter(series.resource_attributes.iter()),
        "service_name": series.service_name,
        "environment": series.environment,
    });
    deterministic_uuid_from_bytes(key.to_string().as_bytes())
}

fn deterministic_uuid_from_bytes(bytes: &[u8]) -> Uuid {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x00000100000001b3;

    let mut hi = FNV_OFFSET;
    let mut lo = FNV_OFFSET ^ 0x9e3779b97f4a7c15;
    for byte in bytes {
        hi ^= u64::from(*byte);
        hi = hi.wrapping_mul(FNV_PRIME);
        lo ^= u64::from(*byte).rotate_left(1);
        lo = lo.wrapping_mul(FNV_PRIME);
    }

    Uuid::from_u128((u128::from(hi) << 64) | u128::from(lo))
}

#[cfg(feature = "storage")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, clickhouse::Row)]
pub struct MetricSeriesRow {
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
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
    #[serde(with = "clickhouse::serde::uuid")]
    pub tenant_id: Uuid,
    #[serde(with = "clickhouse::serde::uuid")]
    pub metric_series_id: Uuid,
    pub metric_name: String,
    pub service_name: String,
    pub time_unix_nano: u64,
    pub start_time_unix_nano: Option<u64>,
    pub value_double: Option<f64>,
    pub value_int: Option<i64>,
    pub histogram_count: Option<u64>,
    pub histogram_sum: Option<f64>,
    pub histogram_bucket_counts: Vec<u64>,
    pub histogram_explicit_bounds: Vec<f64>,
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
            histogram_bucket_counts: p.histogram_bucket_counts.unwrap_or_default(),
            histogram_explicit_bounds: p.histogram_explicit_bounds.unwrap_or_default(),
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
            histogram_bucket_counts: non_empty(row.histogram_bucket_counts),
            histogram_explicit_bounds: non_empty(row.histogram_explicit_bounds),
        }
    }
}

#[cfg(feature = "storage")]
fn non_empty<T>(values: Vec<T>) -> Option<Vec<T>> {
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

#[cfg(all(test, feature = "storage"))]
mod tests {
    use super::*;

    #[test]
    fn metric_point_row_uses_empty_arrays_for_absent_histogram_buckets() {
        let row = MetricPointRow::from(MetricPoint::default());

        assert!(row.histogram_bucket_counts.is_empty());
        assert!(row.histogram_explicit_bounds.is_empty());
    }

    #[test]
    fn metric_point_restores_empty_histogram_arrays_as_absent() {
        let point = MetricPoint::from(MetricPointRow {
            tenant_id: Uuid::nil(),
            metric_series_id: Uuid::nil(),
            metric_name: String::new(),
            service_name: String::new(),
            time_unix_nano: 0,
            start_time_unix_nano: None,
            value_double: None,
            value_int: None,
            histogram_count: None,
            histogram_sum: None,
            histogram_bucket_counts: Vec::new(),
            histogram_explicit_bounds: Vec::new(),
        });

        assert_eq!(point.histogram_bucket_counts, None);
        assert_eq!(point.histogram_explicit_bounds, None);
    }

    #[test]
    fn metric_point_preserves_non_empty_histogram_arrays() {
        let point = MetricPoint::from(MetricPointRow {
            tenant_id: Uuid::nil(),
            metric_series_id: Uuid::nil(),
            metric_name: String::new(),
            service_name: String::new(),
            time_unix_nano: 0,
            start_time_unix_nano: None,
            value_double: None,
            value_int: None,
            histogram_count: Some(3),
            histogram_sum: Some(4.5),
            histogram_bucket_counts: vec![1, 2, 3],
            histogram_explicit_bounds: vec![10.0, 20.0],
        });

        assert_eq!(point.histogram_bucket_counts, Some(vec![1, 2, 3]));
        assert_eq!(point.histogram_explicit_bounds, Some(vec![10.0, 20.0]));
    }
}

#[cfg(test)]
mod deterministic_series_id_tests {
    use super::*;

    fn base_series() -> MetricSeries {
        MetricSeries {
            tenant_id: Uuid::nil(),
            metric_name: "http.server.requests".into(),
            metric_type: MetricType::Sum,
            is_monotonic: Some(true),
            aggregation_temporality: Some(AggregationTemporality::Cumulative),
            attributes: [("route".to_string(), "/checkout".to_string())]
                .into_iter()
                .collect(),
            resource_attributes: [("host.name".to_string(), serde_json::json!("node-a"))]
                .into_iter()
                .collect(),
            service_name: "checkout".into(),
            environment: "prod".into(),
            ..Default::default()
        }
    }

    #[test]
    fn deterministic_metric_series_id_is_stable_for_same_series_identity() {
        let first = base_series();
        let second = base_series();

        assert_eq!(
            deterministic_metric_series_id(&first),
            deterministic_metric_series_id(&second)
        );
    }

    #[test]
    fn deterministic_metric_series_id_changes_when_labels_change() {
        let first = base_series();
        let mut second = base_series();
        second
            .attributes
            .insert("route".to_string(), "/cart".to_string());

        assert_ne!(
            deterministic_metric_series_id(&first),
            deterministic_metric_series_id(&second)
        );
    }
}
