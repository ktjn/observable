use domain::{LogRecord, MetricPoint, MetricSeries, Span};
use uuid::Uuid;

pub fn normalise_span(mut span: Span, tenant_id: Uuid) -> Span {
    span.tenant_id = tenant_id;
    if span.duration_ns == 0 {
        span.duration_ns = span
            .end_time_unix_nano
            .saturating_sub(span.start_time_unix_nano);
    }
    span
}

pub fn normalise_log(mut log: LogRecord, tenant_id: Uuid) -> LogRecord {
    log.tenant_id = tenant_id;
    if log.log_id == uuid::Uuid::nil() {
        log.log_id = uuid::Uuid::new_v4();
    }
    log
}

pub fn normalise_metric_series(mut series: MetricSeries, tenant_id: Uuid) -> MetricSeries {
    series.tenant_id = tenant_id;
    series
}

pub fn normalise_metric_point(mut point: MetricPoint, tenant_id: Uuid) -> MetricPoint {
    point.tenant_id = tenant_id;
    point
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Span, StatusCode};
    use uuid::Uuid;

    #[test]
    fn normalise_fills_duration() {
        let span = Span {
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 1_005_000_000,
            duration_ns: 0,
            ..Default::default()
        };
        let envelope_tenant = Uuid::new_v4();
        let out = normalise_span(span, envelope_tenant);
        assert_eq!(out.duration_ns, 5_000_000);
    }

    #[test]
    fn normalise_defaults_unset_status() {
        let span = Span::default();
        let out = normalise_span(span, Uuid::new_v4());
        assert_eq!(out.status_code, StatusCode::Unset);
    }

    #[test]
    fn normalise_span_stamps_envelope_tenant_id() {
        let envelope_tenant = Uuid::new_v4();
        let span = Span {
            tenant_id: Uuid::nil(),
            ..Default::default()
        };
        let out = normalise_span(span, envelope_tenant);
        assert_eq!(out.tenant_id, envelope_tenant);
    }

    #[test]
    fn normalise_log_stamps_envelope_tenant_id() {
        let envelope_tenant = Uuid::new_v4();
        let log = LogRecord {
            tenant_id: Uuid::nil(),
            ..Default::default()
        };
        let out = normalise_log(log, envelope_tenant);
        assert_eq!(out.tenant_id, envelope_tenant);
    }

    #[test]
    fn normalise_metric_series_stamps_envelope_tenant_id() {
        let envelope_tenant = Uuid::new_v4();
        let series = MetricSeries {
            tenant_id: Uuid::nil(),
            ..Default::default()
        };
        let out = normalise_metric_series(series, envelope_tenant);
        assert_eq!(out.tenant_id, envelope_tenant);
    }

    #[test]
    fn normalise_metric_point_stamps_envelope_tenant_id() {
        let envelope_tenant = Uuid::new_v4();
        let point = MetricPoint {
            tenant_id: Uuid::nil(),
            ..Default::default()
        };
        let out = normalise_metric_point(point, envelope_tenant);
        assert_eq!(out.tenant_id, envelope_tenant);
    }
}
