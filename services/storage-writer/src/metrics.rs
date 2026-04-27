use clickhouse::Client;
use domain::{MetricPoint, MetricPointRow, MetricSeries, MetricSeriesRow};

pub async fn insert_metric_series(ch: &Client, series: Vec<MetricSeries>) -> anyhow::Result<()> {
    let mut insert = ch.insert::<MetricSeriesRow>("metric_series").await?;
    for s in series {
        insert.write(&MetricSeriesRow::from(s)).await?;
    }
    insert.end().await?;
    Ok(())
}

pub async fn insert_metric_points(ch: &Client, points: Vec<MetricPoint>) -> anyhow::Result<()> {
    let mut insert = ch.insert::<MetricPointRow>("metric_points").await?;
    for p in points {
        insert.write(&MetricPointRow::from(p)).await?;
    }
    insert.end().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{AggregationTemporality, MetricPoint, MetricSeries, MetricType};
    use uuid::Uuid;

    fn make_series(tenant_id: Uuid) -> MetricSeries {
        MetricSeries {
            tenant_id,
            metric_series_id: Uuid::new_v4(),
            metric_name: "http.request.duration".into(),
            description: "Request duration histogram".into(),
            unit: "ms".into(),
            metric_type: MetricType::Histogram,
            is_monotonic: None,
            aggregation_temporality: Some(AggregationTemporality::Cumulative),
            attributes: [("route".to_string(), "/checkout".to_string())]
                .into_iter()
                .collect(),
            resource_attributes: [("host.name".to_string(), serde_json::json!("web-1"))]
                .into_iter()
                .collect(),
            service_name: "checkout".into(),
            environment: "prod".into(),
        }
    }

    fn make_point(tenant_id: Uuid, series_id: Uuid) -> MetricPoint {
        MetricPoint {
            tenant_id,
            metric_series_id: series_id,
            metric_name: "http.request.duration".into(),
            service_name: "checkout".into(),
            time_unix_nano: 1_700_000_000_000_000_000,
            start_time_unix_nano: Some(1_699_000_000_000_000_000),
            value_double: Some(42.5),
            value_int: None,
            histogram_count: Some(10),
            histogram_sum: Some(425.0),
            histogram_bucket_counts: Some(vec![2, 3, 5]),
            histogram_explicit_bounds: Some(vec![10.0, 50.0]),
        }
    }

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

    #[test]
    fn metric_series_row_preserves_tenant_id() {
        let tenant_id = Uuid::new_v4();
        let series = MetricSeries {
            tenant_id,
            ..Default::default()
        };
        let row = MetricSeriesRow::from(series);
        assert_eq!(row.tenant_id, tenant_id);
    }

    #[test]
    fn metric_point_row_preserves_tenant_id() {
        let tenant_id = Uuid::new_v4();
        let point = MetricPoint {
            tenant_id,
            ..Default::default()
        };
        let row = MetricPointRow::from(point);
        assert_eq!(row.tenant_id, tenant_id);
    }

    #[test]
    fn metric_series_roundtrip_preserves_all_fields() {
        let tenant_id = Uuid::new_v4();
        let original = make_series(tenant_id);
        let row = MetricSeriesRow::from(original.clone());
        let recovered = MetricSeries::from(row);

        assert_eq!(recovered.tenant_id, original.tenant_id);
        assert_eq!(recovered.metric_series_id, original.metric_series_id);
        assert_eq!(recovered.metric_name, original.metric_name);
        assert_eq!(recovered.description, original.description);
        assert_eq!(recovered.unit, original.unit);
        assert_eq!(recovered.metric_type, original.metric_type);
        assert_eq!(recovered.is_monotonic, original.is_monotonic);
        assert_eq!(
            recovered.aggregation_temporality,
            original.aggregation_temporality
        );
        assert_eq!(recovered.attributes, original.attributes);
        assert_eq!(recovered.resource_attributes, original.resource_attributes);
        assert_eq!(recovered.service_name, original.service_name);
        assert_eq!(recovered.environment, original.environment);
    }

    #[test]
    fn metric_series_type_roundtrips_all_variants() {
        for (metric_type, expected_str) in [
            (MetricType::Gauge, "gauge"),
            (MetricType::Sum, "sum"),
            (MetricType::Histogram, "histogram"),
            (MetricType::ExponentialHistogram, "exponential_histogram"),
            (MetricType::Summary, "summary"),
        ] {
            let row = MetricSeriesRow::from(MetricSeries {
                metric_type: metric_type.clone(),
                ..Default::default()
            });
            assert_eq!(row.metric_type, expected_str);
            let recovered = MetricSeries::from(row);
            assert_eq!(recovered.metric_type, metric_type);
        }
    }

    #[test]
    fn metric_point_roundtrip_preserves_all_fields() {
        let tenant_id = Uuid::new_v4();
        let series_id = Uuid::new_v4();
        let original = make_point(tenant_id, series_id);
        let row = MetricPointRow::from(original.clone());
        let recovered = MetricPoint::from(row);

        assert_eq!(recovered.tenant_id, original.tenant_id);
        assert_eq!(recovered.metric_series_id, original.metric_series_id);
        assert_eq!(recovered.metric_name, original.metric_name);
        assert_eq!(recovered.service_name, original.service_name);
        assert_eq!(recovered.time_unix_nano, original.time_unix_nano);
        assert_eq!(
            recovered.start_time_unix_nano,
            original.start_time_unix_nano
        );
        assert_eq!(recovered.value_double, original.value_double);
        assert_eq!(recovered.value_int, original.value_int);
        assert_eq!(recovered.histogram_count, original.histogram_count);
        assert_eq!(recovered.histogram_sum, original.histogram_sum);
        assert_eq!(
            recovered.histogram_bucket_counts,
            original.histogram_bucket_counts
        );
        assert_eq!(
            recovered.histogram_explicit_bounds,
            original.histogram_explicit_bounds
        );
    }
}
