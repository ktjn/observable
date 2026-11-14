//! Phase 2 P1 health assessment & data-quality insights integration tests
//! (OBS-03-05..08, OBS-26-03, OBS-26-05, OBS-26-06).

mod common;

use chrono::Utc;
use common::Harness;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn regional_incident_visible_without_labeling_all_segments_affected() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, _, raw) = h
        .send(
            "PUT",
            &format!("/v1/health/services/{svc_id}/segments"),
            "admin",
            &json!({
                "dimension": "region",
                "segment_value": "eu-west-1",
                "health_state": "degraded",
                "incident_summary": "Elevated 5xx in EU edge"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    let (status, _, raw) = h
        .send(
            "PUT",
            &format!("/v1/health/services/{svc_id}/segments"),
            "admin",
            &json!({
                "dimension": "region",
                "segment_value": "us-east-1",
                "health_state": "normal",
                "incident_summary": ""
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    let (status, body, raw) = h
        .get(
            &format!("/v1/health/services/{svc_id}/segments"),
            Some("member"),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let eu = items
        .iter()
        .find(|i| i["segment_value"] == "eu-west-1")
        .unwrap();
    let us = items
        .iter()
        .find(|i| i["segment_value"] == "us-east-1")
        .unwrap();
    assert_eq!(eu["health_state"], "degraded");
    assert_eq!(us["health_state"], "normal");
}

#[tokio::test]
async fn recurring_degradations_revealed_over_selected_period() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    for minutes_ago in [120i64, 30] {
        sqlx::query(
            "INSERT INTO health.transitions \
             (tenant_id, service_id, environment_class, at, from_state, to_state, evidence_status, rule_id, rule_revision) \
             VALUES ($1, $2, 'production', $3, 'normal', 'degraded', 'present', $4, 1)",
        )
        .bind(h.tenant)
        .bind(svc_id)
        .bind(Utc::now() - chrono::Duration::minutes(minutes_ago))
        .bind(Uuid::new_v4())
        .execute(&h.state.db)
        .await
        .unwrap();
    }

    let (status, body, raw) = h
        .get(
            &format!("/v1/health/services/{svc_id}/recurring?days=30"),
            Some("member"),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["degradation_count"], 2);
    assert_eq!(body["recurring"], true);
}

#[tokio::test]
async fn dependency_contribution_explains_why_component_degrades_or_not() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, _, raw) = h
        .send(
            "PUT",
            &format!("/v1/health/services/{svc_id}/dependencies/explain"),
            "admin",
            &json!({
                "dependency_name": "redis-cache",
                "contribution_mode": "non_blocking",
                "rationale": "Falls back to primary DB on cache miss"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    let (status, _, raw) = h
        .send(
            "PUT",
            &format!("/v1/health/services/{svc_id}/dependencies/explain"),
            "admin",
            &json!({
                "dependency_name": "postgres-primary",
                "contribution_mode": "critical_path",
                "rationale": "All writes require primary"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    let (status, body, raw) = h
        .get(
            &format!("/v1/health/services/{svc_id}/dependencies/explain"),
            Some("member"),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    let items = body["items"].as_array().unwrap();
    let cache = items
        .iter()
        .find(|i| i["dependency_name"] == "redis-cache")
        .unwrap();
    let pg = items
        .iter()
        .find(|i| i["dependency_name"] == "postgres-primary")
        .unwrap();
    assert_eq!(cache["impacts_service_health"], false);
    assert_eq!(pg["impacts_service_health"], true);
}

#[tokio::test]
async fn shared_network_degradation_visible_alongside_supported_services() {
    let h = Harness::start().await;
    let svc_a = Uuid::new_v4();
    let svc_b = Uuid::new_v4();

    let (status, _, raw) = h
        .send(
            "PUT",
            "/v1/health/infrastructure",
            "admin",
            &json!({
                "infra_id": "core-vpc-transit",
                "infra_kind": "network",
                "health_state": "degraded",
                "detail": "Packet loss 1.8% on AZ-1",
                "supported_service_ids": [svc_a, svc_b]
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");

    let (status, body, raw) = h
        .get(
            &format!("/v1/health/infrastructure?service_id={svc_a}"),
            Some("member"),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["infra_id"], "core-vpc-transit");
    assert_eq!(items[0]["health_state"], "degraded");
    assert_eq!(
        items[0]["supported_service_ids"].as_array().unwrap().len(),
        2
    );
}

#[tokio::test]
async fn reviewer_traces_finding_to_evidence_source_and_qualifications() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, _, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/qualifications"),
            "admin",
            &json!({
                "finding_id": "finding-latency-spike-42",
                "evidence_source": "clickhouse.spans",
                "coverage_pct": 25.0,
                "confidence_level": "medium",
                "limitations": "Head-sampled at 25% in edge collector"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 201, "{raw}");

    let (status, body, raw) = h
        .get(
            &format!("/v1/health/services/{svc_id}/qualifications"),
            Some("member"),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["finding_id"], "finding-latency-spike-42");
    assert_eq!(items[0]["evidence_source"], "clickhouse.spans");
    assert_eq!(items[0]["coverage_pct"], 25.0);
}

#[tokio::test]
async fn monthly_review_identifies_unresolved_quality_issues_and_affected_services() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO health.data_quality_issues \
         (id, tenant_id, service_id, environment_class, kind, service_name, environment, opened_at, last_seen_at, detail) \
         VALUES ($1, $2, $3, 'production', 'source_stopped', 'checkout-api', 'prod', now(), now(), $4)",
    )
    .bind(Uuid::new_v4())
    .bind(h.tenant)
    .bind(svc_id)
    .bind(json!({"consequence": "SLO burn rate blind spot"}))
    .execute(&h.state.db)
    .await
    .unwrap();

    let (status, body, raw) = h
        .get("/v1/health/issues/monthly-review", Some("member"))
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["unresolved_count"], 1);
    assert_eq!(body["affected_service_ids"][0], svc_id.to_string());
}

#[tokio::test]
async fn sampled_rare_event_count_is_labeled_as_estimated() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, body, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/sampled-estimate"),
            "member",
            &json!({"observed_count": 3, "sampling_rate": 0.1}).to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["estimated"], true);
    assert_eq!(body["label"], "estimated");
    assert_eq!(body["estimated_total"], 30);
}

#[tokio::test]
async fn performance_regression_segments_by_approved_business_dimension() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (bad_status, _, _) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/performance-segments"),
            "member",
            &json!({"dimension": "unapproved_raw_ip", "samples": []}).to_string(),
        )
        .await;
    assert_eq!(bad_status, 400);

    let (status, body, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/performance-segments"),
            "member",
            &json!({
                "dimension": "customer_group",
                "samples": [
                    {"segment_value": "enterprise", "baseline_p95_ms": 120.0, "observed_p95_ms": 480.0},
                    {"segment_value": "free_tier", "baseline_p95_ms": 110.0, "observed_p95_ms": 115.0}
                ]
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["dimension"], "customer_group");
    assert_eq!(body["segments"][0]["segment_value"], "enterprise");
    assert_eq!(body["segments"][0]["regressed"], true);
}

#[tokio::test]
async fn capacity_readiness_review_identifies_forecast_risk_and_accountable_action() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (missing_action_status, _, _) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/capacity-readiness"),
            "member",
            &json!({
                "peak_event": "Black Friday",
                "forecast_demand_rps": 1800.0,
                "provisioned_capacity_rps": 1500.0,
                "accountable_owner": "checkout-sre",
                "accountable_action": ""
            })
            .to_string(),
        )
        .await;
    assert_eq!(missing_action_status, 422);

    let (status, body, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/capacity-readiness"),
            "member",
            &json!({
                "peak_event": "Black Friday",
                "forecast_demand_rps": 1800.0,
                "provisioned_capacity_rps": 1500.0,
                "accountable_owner": "checkout-sre",
                "accountable_action": "Pre-scale checkout cluster to 2400 rps"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["risk"], "high");
    assert_eq!(
        body["accountable_action"],
        "Pre-scale checkout cluster to 2400 rps"
    );
}

#[tokio::test]
async fn limit_warnings_notify_owner_before_exhaustion() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, body, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/limit-warnings"),
            "member",
            &json!({
                "resource_name": "postgres_connections",
                "current_usage": 850.0,
                "approved_limit": 1000.0,
                "warning_threshold_ratio": 0.8,
                "accountable_owner": "payments-oncall"
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["status"], "approaching_limit");
    assert_eq!(body["warn_owner"], true);
    assert_eq!(body["accountable_owner"], "payments-oncall");
}

#[tokio::test]
async fn performance_attribution_pinpoints_component_operation_and_resource() {
    let h = Harness::start().await;
    let svc_id = Uuid::new_v4();

    let (status, body, raw) = h
        .send(
            "POST",
            &format!("/v1/health/services/{svc_id}/performance-attribution"),
            "member",
            &json!({
                "trace_or_span_id": "span-991",
                "candidates": [
                    {"component": "auth-middleware", "operation": "verify_jwt", "resource": "cpu_ms", "duration_ms": 18.0},
                    {"component": "checkout-db", "operation": "SELECT orders", "resource": "db_wait_ms", "duration_ms": 420.0}
                ]
            })
            .to_string(),
        )
        .await;
    assert_eq!(status, 200, "{raw}");
    assert_eq!(body["primary_component"], "checkout-db");
    assert_eq!(body["primary_operation"], "SELECT orders");
    assert_eq!(body["dominant_resource"], "db_wait_ms");
}
