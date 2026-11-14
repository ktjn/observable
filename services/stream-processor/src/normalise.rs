pub use observable_process::{
    normalise_log, normalise_metric_point, normalise_metric_series, normalise_span,
};

/// Produces a deterministic pseudonymous correlation key without exposing raw customer PII (OBS-14-11).
pub fn pseudonymize_customer_correlation_key(customer_id: &str, salt: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in salt
        .as_bytes()
        .iter()
        .chain(b":")
        .chain(customer_id.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("cust_tok_{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{LogRecord, MetricPoint, MetricSeries, Span, StatusCode};
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

    #[test]
    fn prohibited_sample_data_is_scrubbed_during_normalization() {
        use domain::SpanEvent;
        use std::collections::HashMap;

        let tenant_id = Uuid::new_v4();
        let prohibited = [
            "alice@example.com",
            "123-45-6789",
            "4111 1111 1111 1111",
            "sk_live_51SecretSampleKey9999",
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjMifQ.sig",
            "super-secret-password-value",
        ];

        let span = normalise_span(
            Span {
                service_name: "checkout".into(),
                status_message: format!(
                    "failed for {} with token Bearer {}",
                    prohibited[0], prohibited[4]
                ),
                attributes: HashMap::from([
                    ("user.email".into(), serde_json::json!(prohibited[0])),
                    (
                        "http.query".into(),
                        serde_json::json!(format!("ssn={}", prohibited[1])),
                    ),
                ]),
                resource_attributes: HashMap::from([(
                    "api_key".into(),
                    serde_json::json!(prohibited[3]),
                )]),
                events: vec![SpanEvent {
                    timestamp_unix_nano: 100,
                    name: format!("charged {}", prohibited[2]),
                    attributes: HashMap::from([(
                        "password".into(),
                        serde_json::json!(prohibited[5]),
                    )]),
                    ..Default::default()
                }],
                ..Default::default()
            },
            tenant_id,
        );

        let log = normalise_log(
            LogRecord {
                body: serde_json::json!({
                    "message": format!("auth Bearer {} for {}", prohibited[4], prohibited[0]),
                    "ssn": prohibited[1],
                }),
                attributes: HashMap::from([(
                    "payment.card".into(),
                    serde_json::json!(prohibited[2]),
                )]),
                ..Default::default()
            },
            tenant_id,
        );

        let series = normalise_metric_series(
            MetricSeries {
                attributes: HashMap::from([("secret".into(), prohibited[3].into())]),
                ..Default::default()
            },
            tenant_id,
        );

        let serialized = serde_json::to_string(&(&span, &log, &series)).unwrap();
        for sample in prohibited {
            assert!(
                !serialized.contains(sample),
                "prohibited sample data leaked into normalized output: {sample}"
            );
        }
        assert!(span.status_message.contains("[REDACTED_EMAIL]"));
        assert!(span.status_message.contains("Bearer [REDACTED]"));
        assert_eq!(span.attributes["user.email"], "[REDACTED]");
        assert_eq!(log.attributes["payment.card"], "[REDACTED]");
    }

    #[test]
    fn seeded_secrets_and_pii_are_not_retrievable_after_normalization() {
        use domain::{EnvelopePayload, TelemetryEnvelope};
        use observable_process::merge_telemetry;
        use std::collections::HashMap;

        let tenant_id = Uuid::new_v4();
        let seeded_secrets = [
            "ghp_SeededGitHubToken1234567890",
            "AKIAIOSFODNN7EXAMPLEKEY",
            "obs_seeded_api_token_abcdef",
            "bob.private@corp.example.org",
            "987-65-4321",
        ];

        let merged = merge_telemetry([
            TelemetryEnvelope {
                envelope_id: Uuid::new_v4(),
                tenant_id,
                environment: "production".into(),
                received_at_unix_nano: 1,
                payload: EnvelopePayload::Spans(vec![Span {
                    service_name: "auth".into(),
                    attributes: HashMap::from([
                        (
                            "authorization".into(),
                            serde_json::json!(format!("Bearer {}", seeded_secrets[0])),
                        ),
                        (
                            "detail".into(),
                            serde_json::json!(format!(
                                "aws={} obs={}",
                                seeded_secrets[1], seeded_secrets[2]
                            )),
                        ),
                    ]),
                    ..Default::default()
                }]),
            },
            TelemetryEnvelope {
                envelope_id: Uuid::new_v4(),
                tenant_id,
                environment: "production".into(),
                received_at_unix_nano: 2,
                payload: EnvelopePayload::Logs(vec![LogRecord {
                    body: serde_json::json!(format!(
                        "user {} ssn {} failed",
                        seeded_secrets[3], seeded_secrets[4]
                    )),
                    ..Default::default()
                }]),
            },
        ]);

        let downstream_wire = serde_json::to_string(&(&merged.spans, &merged.logs)).unwrap();
        for secret in seeded_secrets {
            assert!(
                !downstream_wire.contains(secret),
                "seeded secret/PII remained retrievable in normalized payload: {secret}"
            );
        }
    }

    #[test]
    fn obs_14_11_correlates_customer_events_without_exposing_identifying_details() {
        let token_a1 =
            pseudonymize_customer_correlation_key("cust-alice@example.com", "tenant-salt");
        let token_a2 =
            pseudonymize_customer_correlation_key("cust-alice@example.com", "tenant-salt");
        let token_b = pseudonymize_customer_correlation_key("cust-bob@example.com", "tenant-salt");
        assert_eq!(token_a1, token_a2);
        assert_ne!(token_a1, token_b);
        assert!(!token_a1.contains("alice"));
    }
}
