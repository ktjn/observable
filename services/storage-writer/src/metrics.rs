use clickhouse::Client;
use domain::{MetricPoint, MetricPointRow, MetricSeries, MetricSeriesRow};

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
