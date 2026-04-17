use domain::{LogRecord, Span};

pub fn normalise_span(mut span: Span) -> Span {
    if span.duration_ns == 0 {
        span.duration_ns = span
            .end_time_unix_nano
            .saturating_sub(span.start_time_unix_nano);
    }
    span
}

pub fn normalise_log(mut log: LogRecord) -> LogRecord {
    if log.log_id == uuid::Uuid::nil() {
        log.log_id = uuid::Uuid::new_v4();
    }
    log
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Span, StatusCode};
    use uuid::Uuid;

    #[test]
    fn normalise_fills_duration() {
        let span = Span {
            tenant_id: Uuid::new_v4(),
            start_time_unix_nano: 1_000_000_000,
            end_time_unix_nano: 1_005_000_000,
            duration_ns: 0,
            ..Default::default()
        };
        let out = normalise_span(span);
        assert_eq!(out.duration_ns, 5_000_000);
    }

    #[test]
    fn normalise_defaults_unset_status() {
        let span = Span {
            tenant_id: Uuid::new_v4(),
            ..Default::default()
        };
        let out = normalise_span(span);
        assert_eq!(out.status_code, StatusCode::Unset);
    }
}
