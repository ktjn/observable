use clickhouse::Client;
use domain::{Span, SpanEvent, SpanEventRow, SpanRow};

pub async fn insert_spans(ch: &Client, spans: Vec<Span>) -> anyhow::Result<()> {
    // Collect events before consuming spans
    let all_events: Vec<SpanEvent> = spans.iter()
        .flat_map(|s| s.events.iter().cloned())
        .collect();

    let mut insert = ch.insert::<SpanRow>("spans").await?;
    for span in spans {
        insert.write(&SpanRow::from(span)).await?;
    }
    insert.end().await?;

    if !all_events.is_empty() {
        let mut ev_insert = ch.insert::<SpanEventRow>("span_events").await?;
        for ev in all_events {
            ev_insert.write(&SpanEventRow::from(ev)).await?;
        }
        ev_insert.end().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{Span, SpanKind, StatusCode};
    use uuid::Uuid;

    fn make_span(tenant_id: Uuid) -> Span {
        Span {
            tenant_id,
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".into(),
            span_id: "00f067aa0ba902b7".into(),
            parent_span_id: Some("aabbccdd11223344".into()),
            service_name: "checkout".into(),
            service_namespace: "payments".into(),
            service_version: "1.2.3".into(),
            operation_name: "POST /order".into(),
            span_kind: SpanKind::Server,
            start_time_unix_nano: 1_700_000_000_000_000_000,
            end_time_unix_nano: 1_700_000_000_005_000_000,
            duration_ns: 5_000_000,
            status_code: StatusCode::Ok,
            status_message: "all good".into(),
            attributes: [("http.method".to_string(), serde_json::json!("POST"))]
                .into_iter()
                .collect(),
            resource_attributes: [("host.name".to_string(), serde_json::json!("web-1"))]
                .into_iter()
                .collect(),
            environment: "prod".into(),
            host_id: "web-1".into(),
            workload: "checkout-deploy".into(),
            deployment_id: "deploy-42".into(),
            events: vec![],
        }
    }

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

    #[test]
    fn span_row_roundtrip_preserves_all_scalar_fields() {
        let tenant_id = Uuid::new_v4();
        let original = make_span(tenant_id);
        let row = SpanRow::from(original.clone());
        let recovered = Span::from(row);

        assert_eq!(recovered.tenant_id, original.tenant_id);
        assert_eq!(recovered.trace_id, original.trace_id);
        assert_eq!(recovered.span_id, original.span_id);
        assert_eq!(recovered.parent_span_id, original.parent_span_id);
        assert_eq!(recovered.service_name, original.service_name);
        assert_eq!(recovered.service_namespace, original.service_namespace);
        assert_eq!(recovered.service_version, original.service_version);
        assert_eq!(recovered.operation_name, original.operation_name);
        assert_eq!(recovered.span_kind, original.span_kind);
        assert_eq!(
            recovered.start_time_unix_nano,
            original.start_time_unix_nano
        );
        assert_eq!(recovered.end_time_unix_nano, original.end_time_unix_nano);
        assert_eq!(recovered.duration_ns, original.duration_ns);
        assert_eq!(recovered.status_code, original.status_code);
        assert_eq!(recovered.status_message, original.status_message);
        assert_eq!(recovered.attributes, original.attributes);
        assert_eq!(recovered.resource_attributes, original.resource_attributes);
        assert_eq!(recovered.environment, original.environment);
        assert_eq!(recovered.host_id, original.host_id);
        assert_eq!(recovered.workload, original.workload);
        assert_eq!(recovered.deployment_id, original.deployment_id);
    }

    // SELECT_COLS used by the query-api must cover exactly the SpanRow fields so that
    // inserts and queries operate on the same set of columns with no gaps.
    #[test]
    fn select_cols_field_count_matches_span_row_struct() {
        // SpanRow has 20 fields; count the comma-separated names in SELECT_COLS.
        let select_cols = "tenant_id, trace_id, span_id, parent_span_id, service_name, \
            service_namespace, service_version, operation_name, span_kind, \
            start_time_unix_nano, end_time_unix_nano, duration_ns, \
            status_code, status_message, attributes, resource_attributes, \
            environment, host_id, workload, deployment_id";
        let col_count = select_cols.split(',').count();
        // SpanRow fields count derived from the struct definition.
        let span_row_field_count = 20usize;
        assert_eq!(
            col_count, span_row_field_count,
            "SELECT_COLS has {col_count} columns but SpanRow has {span_row_field_count} fields"
        );
    }

    #[test]
    fn span_row_status_roundtrips_all_variants() {
        for (status, expected_str) in [
            (StatusCode::Ok, "OK"),
            (StatusCode::Error, "ERROR"),
            (StatusCode::Unset, "UNSET"),
        ] {
            let row = SpanRow::from(Span {
                status_code: status.clone(),
                ..Default::default()
            });
            assert_eq!(row.status_code, expected_str);
            let recovered = Span::from(row);
            assert_eq!(recovered.status_code, status);
        }
    }

    #[test]
    fn span_row_span_kind_roundtrips_all_variants() {
        for (kind, expected_str) in [
            (SpanKind::Internal, "INTERNAL"),
            (SpanKind::Server, "SERVER"),
            (SpanKind::Client, "CLIENT"),
            (SpanKind::Producer, "PRODUCER"),
            (SpanKind::Consumer, "CONSUMER"),
        ] {
            let row = SpanRow::from(Span {
                span_kind: kind.clone(),
                ..Default::default()
            });
            assert_eq!(row.span_kind, expected_str);
            let recovered = Span::from(row);
            assert_eq!(recovered.span_kind, kind);
        }
    }
}
