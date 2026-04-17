use clickhouse::{Client, Row};
use domain::{AggregationTemporality, MetricPoint, MetricSeries, MetricType};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Row, Serialize, Deserialize)]
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

impl From<MetricSeries> for MetricSeriesRow {
    fn from(s: MetricSeries) -> Self {
        Self {
            tenant_id: s.tenant_id,
            metric_series_id: s.metric_series_id,
            metric_name: s.metric_name,
            description: s.description,
            unit: s.unit,
            metric_type: match s.metric_type {
                MetricType::Gauge => "gauge",
                MetricType::Sum => "sum",
                MetricType::Histogram => "histogram",
                MetricType::ExponentialHistogram => "exponential_histogram",
                MetricType::Summary => "summary",
            }
            .into(),
            is_monotonic: s.is_monotonic.map(|b| if b { 1u8 } else { 0u8 }),
            aggregation_temporality: s.aggregation_temporality.map(|t| match t {
                AggregationTemporality::Delta => "delta".into(),
                AggregationTemporality::Cumulative => "cumulative".into(),
            }),
            attributes: serde_json::to_string(&s.attributes).unwrap_or_default(),
            resource_attributes: serde_json::to_string(&s.resource_attributes).unwrap_or_default(),
            service_name: s.service_name,
            environment: s.environment,
        }
    }
}

#[derive(Debug, Row, Serialize, Deserialize)]
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
    pub histogram_bucket_counts: Vec<u64>,
    pub histogram_explicit_bounds: Vec<f64>,
}

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

pub async fn insert_metric_series(ch: &Client, series: Vec<MetricSeries>) -> anyhow::Result<()> {
    let mut insert = ch.insert("metric_series")?;
    for s in series {
        insert.write(&MetricSeriesRow::from(s)).await?;
    }
    insert.end().await?;
    Ok(())
}

pub async fn insert_metric_points(ch: &Client, points: Vec<MetricPoint>) -> anyhow::Result<()> {
    let mut insert = ch.insert("metric_points")?;
    for p in points {
        insert.write(&MetricPointRow::from(p)).await?;
    }
    insert.end().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{MetricSeries, MetricType};
    use uuid::Uuid;

    #[test]
    fn metric_series_row_maps_type() {
        let series = MetricSeries {
            tenant_id: Uuid::new_v4(),
            metric_series_id: Uuid::new_v4(),
            metric_name: "http.request.duration".into(),
            metric_type: MetricType::Histogram,
            service_name: "checkout".into(),
            ..Default::default()
        };
        let row = MetricSeriesRow::from(series);
        assert_eq!(row.metric_type, "histogram");
    }
}
