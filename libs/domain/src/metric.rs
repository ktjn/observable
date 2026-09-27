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

/// Canonical metric data point. Mirrors `metrics.MetricPoint@1` in
/// `models/metrics.mdl` field-for-field.
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
