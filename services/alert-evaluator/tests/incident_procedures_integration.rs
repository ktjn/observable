//! Recorded execution of an incident's approved procedure (OBS-33-05) through the procedure
//! routes on real PostgreSQL, as the alerting login.

use std::sync::Arc;

use alert_evaluator::middleware::auth::TenantContext;
use alert_evaluator::{AppState, observability, procedures};
use axum::{
    Extension, Router,
    body::Body,
    http::Request,
    routing::{get, post},
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const RUNBOOK: &str = "https://runbooks.example/checkout/restart";

struct World {
    state: AppState,
    owner: PgPool,
    tenant: Uuid,
    admin: Uuid,
    second_admin: Uuid,
    member: Uuid,
    _db: test_support::postgres::TestDatabase,
}

impl World {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        let db = test_support::postgres::scoped_pool(&owner, "observable_alerting").await;
        Self {
            state: AppState {
                db,
                ch: clickhouse::Client::default().with_url("http://127.0.0.1:19999"),
                auth_service_url: "http://unused".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
                producer: None,
            },
            owner,
            tenant: Uuid::new_v4(),
            admin: Uuid::new_v4(),
            second_admin: Uuid::new_v4(),
            member: Uuid::new_v4(),
            _db: guard,
        }
    }

    fn ctx(&self, who: &str) -> TenantContext {
        let (tenant_id, user_id, role) = match who {
            "admin" => (self.tenant, Some(self.admin), "tenant_admin"),
            "admin2" => (self.tenant, Some(self.second_admin), "tenant_admin"),
            "member" => (self.tenant, Some(self.member), "member"),
            "viewer" => (self.tenant, Some(Uuid::new_v4()), "viewer"),
            "automation" => (self.tenant, None, "member"),
            "other" => (Uuid::new_v4(), Some(Uuid::new_v4()), "tenant_admin"),
            _ => unreachable!(),
        };
        TenantContext {
            tenant_id,
            user_id,
            role: role.into(),
        }
    }

    async fn call(&self, who: &str, method: &str, uri: &str, body: Option<Value>) -> (u16, Value) {
        let app = Router::new()
            .route(
                "/v1/incidents/{incident_id}/procedures",
                get(procedures::handle_list).post(procedures::handle_start),
            )
            .route(
                "/v1/incidents/{incident_id}/procedures/{run_id}/approve",
                post(procedures::handle_approve),
            )
            .route(
                "/v1/incidents/{incident_id}/procedures/{run_id}/steps",
                post(procedures::handle_record_step),
            )
            .route(
                "/v1/incidents/{incident_id}/procedures/{run_id}/finish",
                post(procedures::handle_finish),
            )
            .layer(Extension(self.ctx(who)))
            .with_state(self.state.clone());
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let resp = app.oneshot(req.body(body).unwrap()).await.unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn incident(&self, runbook: Option<&str>) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO alerting.incidents (tenant_id, title, severity, status, dedup_key, runbook_url) \
             VALUES ($1, 'Checkout errors', 'critical', 'triggered', $2, $3) RETURNING incident_id",
        )
        .bind(self.tenant)
        .bind(Uuid::new_v4().to_string())
        .bind(runbook)
        .fetch_one(&self.owner)
        .await
        .unwrap()
    }

    async fn start_run(&self, who: &str, incident: Uuid, automated: bool) -> (u16, Value) {
        self.call(
            who,
            "POST",
            &format!("/v1/incidents/{incident}/procedures"),
            Some(json!({"procedure_ref": RUNBOOK, "automated": automated})),
        )
        .await
    }

    async fn step(&self, who: &str, incident: Uuid, run: &str, name: &str) -> (u16, Value) {
        self.call(
            who,
            "POST",
            &format!("/v1/incidents/{incident}/procedures/{run}/steps"),
            Some(json!({
                "name": name,
                "inputs": {"deployment": "checkout", "replicas": 3},
                "outputs": {"restarted": 3},
                "result": "succeeded",
            })),
        )
        .await
    }
}

#[tokio::test]
async fn executed_procedure_records_each_step_actor_and_result() {
    let w = World::start().await;
    let incident = w.incident(Some(RUNBOOK)).await;

    // Manual run: started and stepped by signed-in users.
    let (status, run) = w.start_run("member", incident, false).await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(run["status"], "in_progress");
    let manual = run["run_id"].as_str().unwrap().to_owned();
    assert_eq!(
        w.step("member", incident, &manual, "drain node").await.0,
        201
    );
    assert_eq!(
        w.step("admin", incident, &manual, "restart pods").await.0,
        201
    );
    // Automation cannot act on a run nobody approved for automation.
    assert_eq!(
        w.step("automation", incident, &manual, "scale").await.0,
        403
    );
    let (status, done) = w
        .call(
            "member",
            "POST",
            &format!("/v1/incidents/{incident}/procedures/{manual}/finish"),
            Some(json!({"outcome": "completed"})),
        )
        .await;
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["status"], "completed");
    assert_eq!(w.step("member", incident, &manual, "late").await.0, 409);

    // Automated run: waits for a second admin, then automation records its steps.
    let (status, run) = w.start_run("admin", incident, true).await;
    assert_eq!(status, 201, "{run}");
    assert_eq!(run["status"], "pending_approval");
    let auto = run["run_id"].as_str().unwrap().to_owned();
    assert_eq!(w.step("automation", incident, &auto, "scale").await.0, 409);
    let approve = format!("/v1/incidents/{incident}/procedures/{auto}/approve");
    assert_eq!(w.call("admin", "POST", &approve, None).await.0, 403);
    assert_eq!(w.call("member", "POST", &approve, None).await.0, 403);
    let (status, approved) = w.call("admin2", "POST", &approve, None).await;
    assert_eq!(status, 200, "{approved}");
    assert_eq!(approved["approved_by"], json!(w.second_admin));
    assert_eq!(
        w.step("automation", incident, &auto, "scale out").await.0,
        201
    );

    let (status, list) = w
        .call(
            "viewer",
            "GET",
            &format!("/v1/incidents/{incident}/procedures"),
            None,
        )
        .await;
    assert_eq!(status, 200, "{list}");
    let runs = list["items"].as_array().unwrap();
    assert_eq!(runs.len(), 2);
    let steps = |i: usize| runs[i]["steps"].as_array().unwrap().clone();
    let summary: Vec<(i64, String, String, Value, String)> = steps(0)
        .iter()
        .map(|s| {
            (
                s["seq"].as_i64().unwrap(),
                s["name"].as_str().unwrap().into(),
                s["actor_kind"].as_str().unwrap().into(),
                s["actor_id"].clone(),
                s["result"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (
                1,
                "drain node".into(),
                "user".into(),
                json!(w.member),
                "succeeded".into()
            ),
            (
                2,
                "restart pods".into(),
                "user".into(),
                json!(w.admin),
                "succeeded".into()
            ),
        ]
    );
    assert_eq!(
        steps(0)[0]["inputs"],
        json!({"deployment": "checkout", "replicas": 3})
    );
    assert_eq!(steps(0)[0]["outputs"], json!({"restarted": 3}));
    assert_eq!(runs[0]["procedure_ref"], RUNBOOK);
    assert_eq!(runs[1]["automated"], true);
    assert_eq!(steps(1)[0]["actor_kind"], "automation");
    assert_eq!(steps(1)[0]["actor_id"], Value::Null);
}

#[tokio::test]
async fn only_the_incidents_approved_procedure_runs_in_the_callers_tenant() {
    let w = World::start().await;
    let incident = w.incident(Some(RUNBOOK)).await;
    let bare = w.incident(None).await;
    let (status, body) = w
        .call(
            "member",
            "POST",
            &format!("/v1/incidents/{incident}/procedures"),
            Some(json!({"procedure_ref": "https://elsewhere.example/script"})),
        )
        .await;
    assert_eq!(
        (status, &body["error"]),
        (409, &json!("procedure_not_approved_for_incident"))
    );
    assert_eq!(w.start_run("member", bare, false).await.0, 409);
    assert_eq!(w.start_run("viewer", incident, false).await.0, 403);
    assert_eq!(w.start_run("automation", incident, true).await.0, 403);
    assert_eq!(w.start_run("other", incident, false).await.0, 404);

    let (_, run) = w.start_run("member", incident, false).await;
    let run = run["run_id"].as_str().unwrap().to_owned();
    assert_eq!(w.step("other", incident, &run, "x").await.0, 404);
    assert_eq!(w.step("viewer", incident, &run, "x").await.0, 403);
    let steps = format!("/v1/incidents/{incident}/procedures/{run}/steps");
    for bad in [
        json!({"name": " ", "result": "succeeded"}),
        json!({"name": "x", "result": "done"}),
        json!({"name": "x", "result": "succeeded", "inputs": [1, 2]}),
        json!({"name": "x", "result": "succeeded", "outputs": {"blob": "a".repeat(20_000)}}),
    ] {
        assert_eq!(
            w.call("member", "POST", &steps, Some(bad.clone())).await.0,
            400,
            "{bad}"
        );
    }
    let (status, _) = w
        .call(
            "other",
            "GET",
            &format!("/v1/incidents/{incident}/procedures"),
            None,
        )
        .await;
    assert_eq!(status, 404);
}
