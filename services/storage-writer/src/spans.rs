use clickhouse::Client;
use domain::{Span, SpanRow};

pub async fn insert_spans(ch: &Client, spans: Vec<Span>) -> anyhow::Result<()> {
    let mut insert = ch.insert::<SpanRow>("spans").await?;
    for span in spans {
        insert.write(&SpanRow::from(span)).await?;
    }
    insert.end().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Span, StatusCode};
    use uuid::Uuid;

    #[test]
    fn ch_row_from_span_maps_status() {
        let span = Span {
            tenant_id: Uuid::new_v4(),
            trace_id: "abc".into(),
            span_id: "def".into(),
            status_code: StatusCode::Error,
            start_time_unix_nano: 1_000,
            end_time_unix_nano: 2_000,
            duration_ns: 1_000,
            ..Default::default()
        };
        let row = SpanRow::from(span);
        assert_eq!(row.status_code, "ERROR");
    }

    #[test]
    fn span_row_preserves_tenant_id() {
        let tenant_id = Uuid::new_v4();
        let span = Span {
            tenant_id,
            ..Default::default()
        };
        let row = SpanRow::from(span);
        assert_eq!(row.tenant_id, tenant_id);
    }
}
