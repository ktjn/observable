//! `observe-ai-services` (OBS-35-01..08) through the mounted HTTP routes against
//! ClickHouse (Testcontainers) and the real tenant middleware.

use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
    middleware as axum_middleware,
};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use clickhouse::Client;
use http_body_util::BodyExt;
use observable_storage_contracts::{LogRow, SpanRow};
use query_api::{ai_observability, middleware::auth::require_tenant};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

const SERVICE: &str = "support-assistant";

struct Harness {
    ch: Client,
    tenant: Uuid,
    key: String,
    app: Router,
    _auth: MockServer,
    /// Baseline window is [t0, t0+1h), current window is [t0+1h, t0+2h).
    t0: DateTime<Utc>,
}

async fn harness() -> Harness {
    let ch = test_support::clickhouse::shared_client().await;
    let tenant = Uuid::new_v4();
    let key = format!("key-{tenant}");
    let auth = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/internal/validate"))
        .and(body_partial_json(json!({ "api_key": key })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tenant_id": tenant, "role": "member", "environment": "prod",
        })))
        .mount(&auth)
        .await;
    let app = Router::new()
        .merge(ai_observability::router(ch.clone()))
        .layer(axum_middleware::from_fn(require_tenant))
        .layer(axum::Extension(Arc::new(auth.uri())))
        .layer(axum::Extension(reqwest::Client::new()));
    let start = (Utc::now() - Duration::hours(3)).timestamp();
    let t0 = DateTime::from_timestamp(start - start % 3600, 0).unwrap();
    Harness {
        ch,
        tenant,
        key,
        app,
        _auth: auth,
        t0,
    }
}

fn ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn ns(t: DateTime<Utc>) -> u64 {
    t.timestamp_nanos_opt().unwrap() as u64
}

impl Harness {
    fn baseline(&self, minute: i64) -> DateTime<Utc> {
        self.t0 + Duration::minutes(minute)
    }
    fn current(&self, minute: i64) -> DateTime<Utc> {
        self.t0 + Duration::minutes(60 + minute)
    }
    fn windows(&self) -> String {
        format!(
            "baseline_from={}&baseline_to={}&from={}&to={}",
            ts(self.t0),
            ts(self.t0 + Duration::hours(1)),
            ts(self.t0 + Duration::hours(1)),
            ts(self.t0 + Duration::hours(2)),
        )
    }
    fn window(&self) -> String {
        format!(
            "from={}&to={}",
            ts(self.t0),
            ts(self.t0 + Duration::hours(2))
        )
    }

    async fn get_as(&self, key: &str, uri: &str) -> (StatusCode, Value) {
        let res = self
            .app
            .clone()
            .oneshot(
                Request::get(uri)
                    .header("authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn get(&self, uri: &str) -> (StatusCode, Value) {
        self.get_as(&self.key, uri).await
    }

    async fn span(
        &self,
        tenant: Uuid,
        trace: &str,
        span: &str,
        at: DateTime<Utc>,
        status: &str,
        attrs: Value,
    ) {
        let row = SpanRow {
            tenant_id: tenant,
            trace_id: trace.into(),
            span_id: span.into(),
            parent_span_id: None,
            service_name: SERVICE.into(),
            service_namespace: String::new(),
            service_version: String::new(),
            operation_name: attrs["gen_ai.operation.name"]
                .as_str()
                .unwrap_or("op")
                .into(),
            span_kind: "CLIENT".into(),
            start_time_unix_nano: ns(at),
            end_time_unix_nano: ns(at) + 200_000_000,
            duration_ns: 200_000_000,
            status_code: status.into(),
            status_message: String::new(),
            attributes: attrs.to_string(),
            resource_attributes: "{}".into(),
            environment: "prod".into(),
            host_id: "h".into(),
            workload: String::new(),
            deployment_id: String::new(),
        };
        let mut ins = self.ch.insert::<SpanRow>("spans").await.unwrap();
        ins.write(&row).await.unwrap();
        ins.end().await.unwrap();
    }

    async fn event(
        &self,
        tenant: Uuid,
        trace: &str,
        span: &str,
        at: DateTime<Utc>,
        name: &str,
        attrs: Value,
    ) {
        self.ch
            .query(
                "INSERT INTO observable.span_events \
                 (tenant_id, trace_id, span_id, event_index, name, timestamp_unix_nano, attributes) \
                 VALUES (?, ?, ?, 0, ?, ?, ?)",
            )
            .bind(tenant)
            .bind(trace)
            .bind(span)
            .bind(name)
            .bind(ns(at))
            .bind(attrs.to_string())
            .execute()
            .await
            .unwrap();
    }

    /// An evaluated chat call in this tenant.
    async fn evaluated_chat(
        &self,
        id: &str,
        at: DateTime<Utc>,
        attrs: Value,
        evaluation: &str,
        score: f64,
    ) {
        let mut a = json!({"gen_ai.operation.name": "chat", "gen_ai.request.model": "model-a"});
        a.as_object_mut()
            .unwrap()
            .extend(attrs.as_object().unwrap().clone());
        self.span(self.tenant, id, id, at, "OK", a).await;
        self.event(
            self.tenant,
            id,
            id,
            at,
            ai_observability::EVALUATION_EVENT,
            json!({"gen_ai.evaluation.name": evaluation, "gen_ai.evaluation.score.value": score}),
        )
        .await;
    }
}

fn uniq(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::new_v4().simple())
}

#[tokio::test]
async fn obs_35_01_a_drop_in_evaluated_answer_quality_is_visible() {
    let h = harness().await;
    for (i, s) in [0.92, 0.90, 0.94].iter().enumerate() {
        h.evaluated_chat(
            &uniq("b"),
            h.baseline(i as i64 * 5),
            json!({}),
            "answer_quality",
            *s,
        )
        .await;
    }
    for (i, s) in [0.70, 0.72].iter().enumerate() {
        h.evaluated_chat(
            &uniq("c"),
            h.current(i as i64 * 5),
            json!({}),
            "answer_quality",
            *s,
        )
        .await;
    }
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/quality?evaluation=answer_quality&{}",
            h.windows()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "degraded", "{body}");
    assert_eq!(body["baseline"]["evaluations"], 3);
    assert_eq!(body["current"]["evaluations"], 2);
    assert!(body["drop"].as_f64().unwrap() > 0.15);

    // Without current evaluations the state is unknown, never stable.
    let (_, other) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/quality?evaluation=groundedness&{}",
            h.windows()
        ))
        .await;
    assert_eq!(other["state"], "unknown");
    assert!(other["current"].is_null());
}

#[tokio::test]
async fn obs_35_02_a_consumption_spike_is_attributed_to_one_feature_and_its_owner() {
    let h = harness().await;
    let call = |feature: &str, tokens: u64| {
        json!({
            "gen_ai.operation.name": "chat",
            "gen_ai.usage.input_tokens": tokens,
            "gen_ai.usage.output_tokens": 0,
            "observable.ai.feature": feature,
            "observable.ai.feature.owner": format!("{feature}-team"),
            "observable.ai.usage.cost": 0.5,
        })
    };
    h.span(
        h.tenant,
        &uniq("t"),
        "s1",
        h.baseline(1),
        "OK",
        call("summarize", 1_000),
    )
    .await;
    h.span(
        h.tenant,
        &uniq("t"),
        "s2",
        h.baseline(2),
        "OK",
        call("triage", 1_000),
    )
    .await;
    h.span(
        h.tenant,
        &uniq("t"),
        "s3",
        h.current(1),
        "OK",
        call("summarize", 9_000),
    )
    .await;
    h.span(
        h.tenant,
        &uniq("t"),
        "s4",
        h.current(2),
        "ERROR",
        call("triage", 1_100),
    )
    .await;
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/consumption?{}",
            h.windows()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    let features = body["features"].as_array().unwrap();
    let by = |f: &str| features.iter().find(|x| x["feature"] == f).unwrap().clone();
    assert_eq!(by("summarize")["spike"], "spike", "{body}");
    assert_eq!(by("summarize")["owner"], "summarize-team");
    assert_eq!(by("summarize")["tokens"], 9_000);
    assert_eq!(by("triage")["spike"], "normal");
    assert_eq!(by("triage")["failed_calls"], 1);
    assert!(by("triage")["p95_duration_ms"].as_f64().unwrap() > 0.0);
}

#[tokio::test]
async fn obs_35_03_an_instruction_change_is_shown_with_the_resulting_quality_shift() {
    let h = harness().await;
    for m in [0, 5, 10] {
        h.evaluated_chat(
            &uniq("v13"),
            h.baseline(m),
            json!({"observable.ai.instruction.version": "prompt-v13"}),
            "answer_quality",
            0.9,
        )
        .await;
    }
    for m in [0, 5] {
        h.evaluated_chat(
            &uniq("v14"),
            h.current(m),
            json!({"observable.ai.instruction.version": "prompt-v14"}),
            "answer_quality",
            0.6,
        )
        .await;
    }
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/changes?evaluation=answer_quality&{}",
            h.window()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    let changes = body["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 1, "{body}");
    assert_eq!(changes[0]["from_instruction_version"], "prompt-v13");
    assert_eq!(changes[0]["to_instruction_version"], "prompt-v14");
    assert!((changes[0]["quality_delta"].as_f64().unwrap() + 0.3).abs() < 1e-9);
}

/// Builds one agent interaction: an agent invocation, a retrieval of stale guidance,
/// a successful but wrong tool call, and an evaluated goal outcome.
async fn agent_interaction(h: &Harness, trace: &str, goal_label: &str) {
    let at = h.current(10);
    h.span(
        h.tenant,
        trace,
        "agent",
        at,
        "OK",
        json!({
            "gen_ai.operation.name": "invoke_agent", "gen_ai.agent.name": "db-remediator",
            "gen_ai.input.messages": "[{\"role\":\"user\",\"content\":\"checkout is timing out\"}]",
        }),
    )
    .await;
    h.span(
        h.tenant,
        trace,
        "plan",
        at + Duration::seconds(1),
        "OK",
        json!({
            "gen_ai.operation.name": "chat", "gen_ai.request.model": "model-a",
            "observable.ai.context.sources": "runbook:checkout-db@v1 (stale)",
            "observable.ai.rationale": "matched 'timeout' in runbook:checkout-db@v1",
            "http.url": "https://internal.example/should-not-be-returned",
        }),
    )
    .await;
    h.span(
        h.tenant,
        trace,
        "tool",
        at + Duration::seconds(2),
        "OK",
        json!({
            "gen_ai.operation.name": "execute_tool", "gen_ai.tool.name": "restart_primary_db",
        }),
    )
    .await;
    h.event(
        h.tenant,
        trace,
        "agent",
        at + Duration::seconds(3),
        ai_observability::EVALUATION_EVENT,
        json!({
            "gen_ai.evaluation.name": "goal_outcome", "gen_ai.evaluation.score.label": goal_label,
            "observable.ai.evaluator": "incident-reviewer:alice",
        }),
    )
    .await;
}

#[tokio::test]
async fn obs_35_04_an_investigator_reconstructs_why_an_agent_took_an_incorrect_action() {
    let h = harness().await;
    let trace = uniq("ix");
    agent_interaction(&h, &trace, "failed").await;
    let (status, body) = h.get(&format!("/v1/ai/interactions/{trace}")).await;
    assert_eq!(status, StatusCode::OK);
    let steps = body["steps"].as_array().unwrap();
    assert_eq!(
        steps
            .iter()
            .map(|s| s["operation"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["invoke_agent", "chat", "execute_tool"]
    );
    assert_eq!(
        steps[1]["attributes"]["observable.ai.rationale"],
        "matched 'timeout' in runbook:checkout-db@v1"
    );
    assert_eq!(
        steps[1]["attributes"]["observable.ai.context.sources"],
        "runbook:checkout-db@v1 (stale)"
    );
    assert_eq!(steps[2]["tool"], "restart_primary_db");
    assert!(
        steps[1]["attributes"].get("http.url").is_none(),
        "only AI attributes are returned"
    );

    // Another tenant's key cannot read it, and the answer equals a missing trace.
    let stranger = harness().await;
    let (denied, _) = stranger.get(&format!("/v1/ai/interactions/{trace}")).await;
    let (missing, _) = h
        .get(&format!("/v1/ai/interactions/{}", uniq("none")))
        .await;
    assert_eq!(denied, StatusCode::NOT_FOUND);
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn obs_35_05_a_controlled_test_that_elicits_restricted_content_is_flagged_to_the_owner() {
    let h = harness().await;
    let probe = uniq("probe");
    h.span(
        h.tenant,
        &probe,
        "p",
        h.current(3),
        "OK",
        json!({
            "gen_ai.operation.name": "chat", "observable.ai.feature": "refunds",
            "observable.ai.feature.owner": "payments-ai",
        }),
    )
    .await;
    h.event(h.tenant, &probe, "p", h.current(3), ai_observability::POLICY_VIOLATION_EVENT, json!({
        "observable.ai.policy.kind": "restricted_content", "observable.ai.safety_probe.id": "redteam-pii-01",
    })).await;
    let unowned = uniq("u");
    h.span(
        h.tenant,
        &unowned,
        "u",
        h.current(4),
        "OK",
        json!({"gen_ai.operation.name": "chat"}),
    )
    .await;
    h.event(
        h.tenant,
        &unowned,
        "u",
        h.current(4),
        ai_observability::POLICY_VIOLATION_EVENT,
        json!({
            "observable.ai.policy.kind": "out_of_scope_action",
        }),
    )
    .await;
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/policy-violations?{}",
            h.window()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    let v = body["violations"].as_array().unwrap();
    let flagged = v.iter().find(|x| x["trace_id"] == probe.as_str()).unwrap();
    assert_eq!(flagged["kind"], "restricted_content");
    assert_eq!(flagged["safety_probe_id"], "redteam-pii-01");
    assert_eq!(flagged["owner"], "payments-ai");
    assert_eq!(flagged["flagged_to_owner"], true);
    let orphan = v
        .iter()
        .find(|x| x["trace_id"] == unowned.as_str())
        .unwrap();
    assert_eq!(orphan["flagged_to_owner"], false);
    assert_eq!(body["unowned"], 1);
}

#[tokio::test]
async fn obs_35_06_a_material_shift_in_input_topics_is_reported_for_review() {
    let h = harness().await;
    let topic = |t: &str| json!({"gen_ai.operation.name": "chat", "observable.ai.input.topic": t});
    for (i, t) in ["billing", "billing", "billing", "shipping"]
        .iter()
        .enumerate()
    {
        h.span(
            h.tenant,
            &uniq("b"),
            "s",
            h.baseline(i as i64),
            "OK",
            topic(t),
        )
        .await;
    }
    for (i, t) in ["refund_fraud", "refund_fraud", "refund_fraud", "billing"]
        .iter()
        .enumerate()
    {
        h.span(
            h.tenant,
            &uniq("c"),
            "s",
            h.current(i as i64),
            "OK",
            topic(t),
        )
        .await;
    }
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/input-drift?threshold=0.2&{}",
            h.windows()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "degraded", "{body}");
    let fraud = body["topics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["topic"] == "refund_fraud")
        .unwrap()
        .clone();
    assert_eq!(fraud["review_required"], true);
    assert!((fraud["current_share"].as_f64().unwrap() - 0.75).abs() < 1e-9);
}

#[tokio::test]
async fn obs_35_07_negative_feedback_links_to_the_interaction_that_produced_it() {
    let h = harness().await;
    let trace = uniq("ix");
    h.span(
        h.tenant,
        &trace,
        "answer",
        h.current(5),
        "OK",
        json!({"gen_ai.operation.name": "chat"}),
    )
    .await;
    let log = LogRow {
        tenant_id: h.tenant,
        log_id: Uuid::new_v4(),
        timestamp_unix_nano: ns(h.current(6)),
        observed_timestamp_unix_nano: ns(h.current(6)),
        severity_number: 9,
        severity_text: "INFO".into(),
        body: "the answer was wrong".into(),
        trace_id: Some(trace.clone()),
        span_id: Some("answer".into()),
        attributes: json!({"observable.ai.feedback.sentiment": "negative"}).to_string(),
        resource_attributes: "{}".into(),
        service_name: SERVICE.into(),
        environment: "prod".into(),
        host_id: "h".into(),
        fingerprint: None,
    };
    let mut ins = h.ch.insert::<LogRow>("logs").await.unwrap();
    ins.write(&log).await.unwrap();
    ins.end().await.unwrap();
    let (status, body) = h
        .get(&format!(
            "/v1/ai/services/{SERVICE}/feedback?sentiment=negative&{}",
            h.window()
        ))
        .await;
    assert_eq!(status, StatusCode::OK);
    let fb = &body["feedback"][0];
    assert_eq!(fb["interaction_trace_id"], trace.as_str(), "{body}");
    assert_eq!(fb["interaction_found"], true);
    let (_, ix) = h.get(&format!("/v1/ai/interactions/{trace}")).await;
    assert_eq!(ix["steps"][0]["span_id"], "answer");
}

#[tokio::test]
async fn obs_35_08_goal_outcome_is_reported_independently_of_tool_call_results() {
    let h = harness().await;
    let failed = uniq("ix");
    agent_interaction(&h, &failed, "failed").await;
    let (status, body) = h.get(&format!("/v1/ai/interactions/{failed}")).await;
    assert_eq!(status, StatusCode::OK);
    // Negative: every tool call succeeded, the user objective failed.
    assert_eq!(body["tool_calls"], 1);
    assert_eq!(body["failed_tool_calls"], 0);
    assert_eq!(body["goal_outcome"], "failed");
    assert_eq!(body["goal_evaluator"], "incident-reviewer:alice");

    let achieved = uniq("ix");
    agent_interaction(&h, &achieved, "achieved").await;
    let (_, ok) = h.get(&format!("/v1/ai/interactions/{achieved}")).await;
    assert_eq!(ok["goal_outcome"], "succeeded");
}

#[tokio::test]
async fn ai_routes_reject_missing_credentials_and_bad_windows() {
    let h = harness().await;
    let res = h
        .app
        .clone()
        .oneshot(
            Request::get(format!(
                "/v1/ai/services/{SERVICE}/consumption?{}",
                h.windows()
            ))
            .body(Body::empty())
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let inverted = format!(
        "baseline_from={}&baseline_to={}&from={}&to={}",
        ts(h.t0 + Duration::hours(1)),
        ts(h.t0),
        ts(h.t0),
        ts(h.t0 + Duration::hours(1))
    );
    let (status, _) = h
        .get(&format!("/v1/ai/services/{SERVICE}/consumption?{inverted}"))
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}
