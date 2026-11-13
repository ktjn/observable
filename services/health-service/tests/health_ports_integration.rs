//! Scoped catalog and telemetry ports of the health worker (phase 1 C2.2): the ClickHouse
//! evidence adapter against real ClickHouse and the catalog adapter over HTTP.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    extract::Query,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::get,
};
use health_service::ports::{
    CatalogInputs, EnvironmentClass, Evidence, EvidenceTimes, InputsUnavailable, TelemetryIdentity,
};
use health_service::scope::{Scope, scope};
use serde_json::{Value, json};
use uuid::Uuid;

fn ns(minutes_ago: u64) -> u64 {
    (SystemTime::now() - Duration::from_secs(minutes_ago * 60))
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

fn identity(name: &str, env: &str) -> TelemetryIdentity {
    TelemetryIdentity {
        service_name: name.into(),
        environment: env.into(),
    }
}

#[tokio::test]
async fn clickhouse_evidence_matches_tenant_name_and_exact_environment_read_only() {
    let ch = test_support::clickhouse::shared_client().await;
    let (tenant, other) = (Uuid::new_v4(), Uuid::new_v4());
    let (recent, older, stale) = (ns(2), ns(10), ns(300));
    let exec = |sql: String| {
        let ch = ch.clone();
        async move { ch.query(&sql).execute().await.expect("insert") }
    };
    exec(format!(
        "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
         start_time_unix_nano, end_time_unix_nano, duration_ns, environment) VALUES \
         ('{tenant}', 't1', 's1', 'payments', 'GET /pay', {recent}, {recent}, 1, 'prod'), \
         ('{tenant}', 't2', 's2', 'payments', 'GET /pay', {older}, {older}, 1, 'staging'), \
         ('{tenant}', 't3', 's3', 'payments', 'GET /pay', {recent}, {recent}, 1, ''), \
         ('{other}', 't4', 's4', 'payments', 'GET /pay', {recent}, {recent}, 1, 'prod')"
    ))
    .await;
    exec(format!(
        "INSERT INTO observable.logs (tenant_id, log_id, timestamp_unix_nano, observed_timestamp_unix_nano, \
         severity_number, body, service_name, environment) VALUES \
         ('{tenant}', generateUUIDv4(), {stale}, {stale}, 9, 'old', 'payments', 'prod'), \
         ('{tenant}', generateUUIDv4(), {older}, {older}, 9, 'qa', 'payments', 'staging')"
    ))
    .await;
    let (series, other_series) = (Uuid::new_v4(), Uuid::new_v4());
    exec(format!(
        // Series are deduplicated by (tenant, service, metric, attributes), not environment.
        "INSERT INTO observable.metric_series (tenant_id, metric_series_id, metric_name, metric_type, \
         service_name, environment) VALUES \
         ('{tenant}', '{series}', 'http_requests', 'sum', 'payments', 'prod'), \
         ('{tenant}', '{other_series}', 'queue_depth', 'gauge', 'payments', 'staging')"
    ))
    .await;
    exec(format!(
        "INSERT INTO observable.metric_points (tenant_id, metric_series_id, metric_name, service_name, \
         time_unix_nano, value_double) VALUES \
         ('{tenant}', '{series}', 'http_requests', 'payments', {older}, 1), \
         ('{tenant}', '{other_series}', 'queue_depth', 'payments', {recent}, 1)"
    ))
    .await;

    let evidence = Evidence::clickhouse(ch.clone());
    let since = ns(60);
    // Production and non-production evidence of the same name never mix.
    assert_eq!(
        evidence
            .latest_since(tenant, &identity("payments", "prod"), since)
            .await
            .unwrap(),
        EvidenceTimes {
            traces: Some(recent),
            logs: None,
            metrics: Some(older),
        }
    );
    assert_eq!(
        evidence
            .latest_since(tenant, &identity("payments", "staging"), since)
            .await
            .unwrap(),
        EvidenceTimes {
            traces: Some(older),
            logs: Some(older),
            metrics: Some(recent),
        }
    );
    // An empty environment is not an identity, even though rows with no environment exist.
    assert_eq!(
        evidence
            .latest_since(tenant, &identity("payments", ""), since)
            .await
            .unwrap(),
        EvidenceTimes::default()
    );
    // Another tenant with the same identity sees only its own rows.
    let foreign = evidence
        .latest_since(other, &identity("payments", "prod"), since)
        .await
        .unwrap();
    assert_eq!((foreign.traces, foreign.metrics), (Some(recent), None));
    // Stale data before the horizon is not evidence.
    assert_eq!(
        evidence
            .latest_since(tenant, &identity("payments", "prod"), ns(1))
            .await
            .unwrap(),
        EvidenceTimes::default()
    );

    // The evidence session is read-only.
    let Evidence::ClickHouse(session) = &evidence else {
        unreachable!()
    };
    let write = session
        .query(&format!(
            "INSERT INTO observable.spans (tenant_id, trace_id, span_id, service_name, operation_name, \
             start_time_unix_nano, end_time_unix_nano, duration_ns, environment) VALUES \
             ('{tenant}', 'x', 'x', 'payments', 'forged', {recent}, {recent}, 1, 'prod')"
        ))
        .execute()
        .await;
    assert!(write.is_err(), "evidence session accepted a write");

    let down = Evidence::clickhouse(
        clickhouse::Client::default()
            .with_url("http://127.0.0.1:1")
            .with_compression(clickhouse::Compression::None),
    );
    assert!(
        down.latest_since(tenant, &identity("payments", "prod"), since)
            .await
            .is_err()
    );
}

/// Stand-in for the catalog route: answers per worker token, as the real route validates it.
async fn catalog_stub(tenant: Uuid, service: Uuid, other: Uuid) -> String {
    let app = Router::new().route(
        "/internal/catalog/health-inputs",
        get(
            move |headers: HeaderMap, Query(q): Query<HashMap<String, String>>| async move {
                let token = |h: &str| headers.get(h).and_then(|v| v.to_str().ok());
                if token("x-internal-token") != Some("svc") {
                    return StatusCode::UNAUTHORIZED.into_response();
                }
                let requested = q.get("tenant_id").cloned().unwrap_or_default();
                match token("x-health-worker-token") {
                    Some("ohw_tenant") if requested == tenant.to_string() => Json(json!({
                        "tenant_id": tenant,
                        "services": [{
                            "id": service, "lifecycle": "live", "criticality": "critical",
                            "classification": "restricted",
                            "bindings": [
                                {"service_name": "payments", "environment": "prod", "environment_class": "production"},
                                {"service_name": "payments", "environment": "", "environment_class": null},
                            ],
                        }],
                    }))
                    .into_response(),
                    // A misbehaving provider answering for a different tenant.
                    Some("ohw_confused") => Json(json!({"tenant_id": other, "services": []}))
                        .into_response(),
                    Some("ohw_down") => (
                        StatusCode::SERVICE_UNAVAILABLE,
                        Json(json!({"error": "control_plane_unavailable"})),
                    )
                        .into_response(),
                    _ => StatusCode::UNAUTHORIZED.into_response(),
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{addr}")
}

fn http_port(base: &str, internal: &str, workers: &[(Uuid, &str)]) -> CatalogInputs {
    CatalogInputs::Http {
        base_url: Arc::new(base.into()),
        internal_token: Arc::new(internal.into()),
        workers: Arc::new(workers.iter().map(|(t, w)| (*t, (*w).into())).collect()),
        client: reqwest::Client::new(),
    }
}

#[tokio::test]
async fn http_catalog_port_reads_only_with_the_tenants_own_worker_credential() {
    let (tenant, other, third, service) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let base = catalog_stub(tenant, service, other).await;
    let port = http_port(
        &base,
        "svc",
        &[
            (tenant, "ohw_tenant"),
            (other, "ohw_tenant"),
            (third, "ohw_confused"),
        ],
    );

    let inputs = port.inputs(tenant).await.expect("inputs");
    assert_eq!(inputs.tenant_id, tenant);
    assert_eq!(
        scope(&inputs, service, EnvironmentClass::Production),
        Scope::Bound(vec![identity("payments", "prod")])
    );
    assert_eq!(
        scope(&inputs, service, EnvironmentClass::NonProduction),
        Scope::Bound(vec![])
    );

    // A credential configured under the wrong tenant is refused by the provider; a tenant with
    // no credential is never requested; a response for another tenant is discarded.
    assert_eq!(port.inputs(other).await, Err(InputsUnavailable));
    assert_eq!(port.inputs(Uuid::new_v4()).await, Err(InputsUnavailable));
    assert_eq!(port.inputs(third).await, Err(InputsUnavailable));

    // Catalog outage, a wrong service token and an unreachable catalog are all unavailable.
    let down = http_port(&base, "svc", &[(tenant, "ohw_down")]);
    assert_eq!(down.inputs(tenant).await, Err(InputsUnavailable));
    let wrong = http_port(&base, "wrong", &[(tenant, "ohw_tenant")]);
    assert_eq!(wrong.inputs(tenant).await, Err(InputsUnavailable));
    let gone = http_port("http://127.0.0.1:1", "svc", &[(tenant, "ohw_tenant")]);
    assert_eq!(gone.inputs(tenant).await, Err(InputsUnavailable));
    // An unavailable catalog leaves the scope undecided rather than empty.
    let substitute = CatalogInputs::substitute();
    substitute.set_unavailable();
    assert!(substitute.inputs(tenant).await.is_err());
}

#[test]
fn malformed_classes_are_not_accepted_as_inputs() {
    let raw: Value = json!({
        "tenant_id": Uuid::new_v4(),
        "services": [{"id": Uuid::new_v4(), "lifecycle": "live", "criticality": "critical",
                      "classification": "internal",
                      "bindings": [{"service_name": "p", "environment": "prod",
                                    "environment_class": "unclassified"}]}],
    });
    assert!(serde_json::from_value::<health_service::ports::TenantInputs>(raw).is_err());
}
