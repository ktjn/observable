//! Operational guidance graph (OBS-39-01..03, 06..14; docs/operational-guidance-graph.md)
//! through the guidance routes, on real PostgreSQL as the alerting login and real ClickHouse,
//! with the catalog and control feeds and catalog authorization substituted.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use alert_evaluator::guidance::{
    self, CatalogFeed, FeedBinding, FeedDeployment, FeedJourneyMember, FeedNamed, FeedOwner,
    FeedOwnerConflict, FeedService, GuidanceAccess, GuidanceSources, StaticSources,
};
use alert_evaluator::middleware::auth::TenantContext;
use alert_evaluator::{AppState, observability};
use axum::{Extension, Router, body::Body, http::Request, routing::get};
use chrono::{DateTime, Duration, Utc};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

struct World {
    state: AppState,
    owner: PgPool,
    tenant: Uuid,
    sources: GuidanceSources,
    feeds: Arc<Mutex<StaticSources>>,
    access: GuidanceAccess,
    hidden: Arc<Mutex<HashSet<Uuid>>>,
    _db: test_support::postgres::TestDatabase,
}

fn service(id: Uuid, name: &str) -> FeedService {
    FeedService {
        id,
        name: name.into(),
    }
}

fn binding(id: Uuid, name: &str, env: &str) -> FeedBinding {
    FeedBinding {
        service_id: id,
        service_name: name.into(),
        environment: env.into(),
    }
}

fn owner(service_id: Uuid, principal: Uuid, at: DateTime<Utc>) -> FeedOwner {
    FeedOwner {
        service_id,
        role: "operational".into(),
        principal_kind: "user".into(),
        principal_id: principal,
        assigned_at: at,
    }
}

fn member(journey_id: Uuid, service_id: Uuid) -> FeedJourneyMember {
    FeedJourneyMember {
        journey_id,
        service_id,
    }
}

impl World {
    async fn start() -> Self {
        let (owner, guard) = test_support::postgres::isolated_pool().await;
        let db = test_support::postgres::scoped_pool(&owner, "observable_alerting").await;
        let ch = test_support::clickhouse::shared_client().await;
        let (sources, feeds) = GuidanceSources::substitute();
        let (access, hidden) = GuidanceAccess::substitute();
        Self {
            state: AppState {
                db,
                ch,
                auth_service_url: "http://unused".into(),
                http_client: reqwest::Client::new(),
                metrics: Arc::new(observability::AlertEvaluatorMetrics::new()),
                producer: None,
            },
            owner,
            tenant: Uuid::new_v4(),
            sources,
            feeds,
            access,
            hidden,
            _db: guard,
        }
    }

    fn catalog(&self, feed: CatalogFeed) {
        self.feeds.lock().unwrap().catalog.insert(self.tenant, feed);
    }

    fn deployments(&self, deps: Vec<FeedDeployment>) {
        self.feeds
            .lock()
            .unwrap()
            .deployments
            .insert(self.tenant, deps);
    }

    fn hide(&self, id: Uuid) {
        self.hidden.lock().unwrap().insert(id);
    }

    async fn sync(&self, at: DateTime<Utc>) {
        guidance::sync_tenant_at(
            &self.state.db,
            &self.state.ch,
            &self.sources,
            self.tenant,
            at,
        )
        .await
        .expect("sync");
    }

    async fn get_as(&self, tenant: Uuid, uri: &str) -> (u16, Value, String) {
        let app = Router::new()
            .route("/v1/guidance/entities", get(guidance::handle_lookup))
            .route(
                "/v1/guidance/entities/{entity_id}",
                get(guidance::handle_entity),
            )
            .route("/v1/guidance/graph", get(guidance::handle_graph))
            .layer(Extension(TenantContext {
                tenant_id: tenant,
                user_id: Some(Uuid::new_v4()),
                role: "member".into(),
            }))
            .layer(Extension(self.access.clone()))
            .with_state(self.state.clone());
        let resp = app
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status().as_u16();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
            text,
        )
    }

    async fn get(&self, uri: &str) -> (u16, Value) {
        let (s, v, _) = self.get_as(self.tenant, uri).await;
        (s, v)
    }

    async fn entity_id(&self, kind: &str, key: impl ToString) -> Uuid {
        sqlx::query_scalar(
            "SELECT entity_id FROM alerting.guidance_entities \
             WHERE tenant_id = $1 AND kind = $2 AND canonical_key = $3",
        )
        .bind(self.tenant)
        .bind(kind)
        .bind(key.to_string())
        .fetch_one(&self.owner)
        .await
        .unwrap_or_else(|_| panic!("no {kind} entity"))
    }

    async fn incident(
        &self,
        title: &str,
        at: DateTime<Utc>,
        services: &[&str],
        owner: &str,
    ) -> Uuid {
        sqlx::query_scalar(
            "INSERT INTO alerting.incidents \
             (tenant_id, title, severity, status, dedup_key, triggered_at, affected_services, owner) \
             VALUES ($1, $2, 'critical', 'triggered', $3, $4, $5, $6) RETURNING incident_id",
        )
        .bind(self.tenant)
        .bind(title)
        .bind(Uuid::new_v4().to_string())
        .bind(at)
        .bind(services.iter().map(|s| s.to_string()).collect::<Vec<_>>())
        .bind(owner)
        .fetch_one(&self.owner)
        .await
        .unwrap()
    }

    /// One trace in which `caller` (name, env) calls `callee`, `age_s` seconds ago.
    async fn call_span(&self, caller: (&str, &str), callee: (&str, &str), age_s: i64) {
        let at = (Utc::now() - Duration::seconds(age_s))
            .timestamp_nanos_opt()
            .unwrap();
        let trace = Uuid::new_v4().simple().to_string();
        let parent = Uuid::new_v4().simple().to_string();
        let child = Uuid::new_v4().simple().to_string();
        let t = self.tenant;
        self.state
            .ch
            .query(&format!(
                "INSERT INTO observable.spans (tenant_id, trace_id, span_id, parent_span_id, \
                 service_name, operation_name, start_time_unix_nano, end_time_unix_nano, \
                 duration_ns, environment, status_code) VALUES \
                 ('{t}', '{trace}', '{parent}', NULL, '{}', 'GET /', {at}, {at}, 1, '{}', 'OK'), \
                 ('{t}', '{trace}', '{child}', '{parent}', '{}', 'POST /pay', {at}, {at}, 1, '{}', 'OK')",
                caller.0, caller.1, callee.0, callee.1
            ))
            .execute()
            .await
            .expect("insert spans");
    }
}

fn edges_of<'a>(body: &'a Value, relation: &str) -> Vec<&'a Value> {
    body["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["relation"] == relation)
        .collect()
}

fn keys(nodes: &Value) -> HashSet<(String, String)> {
    nodes
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["kind"].as_str().unwrap().to_owned(),
                n["canonical_key"].as_str().unwrap().to_owned(),
            )
        })
        .collect()
}

fn at(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
        .replace('+', "%2B")
}

// OBS-39-01, OBS-39-07, OBS-39-08
#[tokio::test]
async fn responder_follows_a_journey_to_service_change_and_owner_without_hidden_services() {
    let w = World::start().await;
    let (checkout, ledger, journey, oncall) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let now = Utc::now();
    w.catalog(CatalogFeed {
        services: vec![service(checkout, "Checkout"), service(ledger, "Ledger")],
        bindings: vec![
            binding(checkout, "checkout", "prod"),
            binding(ledger, "ledger", "prod"),
        ],
        journeys: vec![FeedNamed {
            id: journey,
            name: "Buy".into(),
        }],
        journey_services: vec![member(journey, checkout), member(journey, ledger)],
        owners: vec![
            owner(checkout, oncall, now - Duration::days(5)),
            owner(ledger, Uuid::new_v4(), now - Duration::days(5)),
        ],
        ..Default::default()
    });
    let (deploy, hidden_deploy) = (Uuid::new_v4(), Uuid::new_v4());
    w.deployments(vec![
        FeedDeployment {
            deployment_id: deploy,
            service_name: "checkout".into(),
            environment: "prod".into(),
            service_version: "1.4.2".into(),
            status: "succeeded".into(),
            started_at: now - Duration::hours(1),
        },
        FeedDeployment {
            deployment_id: hidden_deploy,
            service_name: "ledger".into(),
            environment: "prod".into(),
            service_version: "9.0.0".into(),
            status: "succeeded".into(),
            started_at: now - Duration::minutes(30),
        },
    ]);
    w.sync(now).await;
    w.hide(ledger);

    let (status, found) = w
        .get(&format!("/v1/guidance/entities?kind=journey&key={journey}"))
        .await;
    assert_eq!(status, 200, "{found}");
    let j = found["entity_id"].as_str().unwrap().to_owned();

    let (status, graph) = w.get(&format!("/v1/guidance/graph?from={j}&depth=3")).await;
    assert_eq!(status, 200, "{graph}");
    // Journey -> service -> recent change, and service -> owner.
    assert_eq!(
        keys(&graph["nodes"]),
        HashSet::from([
            ("journey".into(), journey.to_string()),
            ("service".into(), checkout.to_string()),
            ("change".into(), deploy.to_string()),
            ("owner".into(), format!("user:{oncall}")),
        ])
    );
    let s = w.entity_id("service", checkout).await.to_string();
    let rels: HashSet<(String, String, String)> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["relation"].as_str().unwrap().into(),
                e["subject_id"].as_str().unwrap().into(),
                e["source"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert_eq!(
        rels,
        HashSet::from([
            ("part_of_journey".into(), s.clone(), "catalog".into()),
            ("changed_by".into(), s.clone(), "control".into()),
            ("owned_by".into(), s.clone(), "catalog".into()),
        ])
    );
    // The hidden service is absent with every edge and its change; nothing counts it.
    let hidden = w.entity_id("service", ledger).await.to_string();
    assert!(!graph.to_string().contains(&hidden), "{graph}");
    assert!(!graph.to_string().contains(&hidden_deploy.to_string()));
    assert_eq!(graph["edges"].as_array().unwrap().len(), 3);

    // No telemetry and no declared dependency: calls is unknown, never an empty known set.
    let (status, detail) = w.get(&format!("/v1/guidance/entities/{s}")).await;
    assert_eq!(status, 200, "{detail}");
    assert_eq!(detail["relations"]["calls"], "unknown");
    assert_eq!(detail["relations"]["provided_by"], "unknown");
    assert_eq!(detail["relations"]["owned_by"], "known");
    assert_eq!(detail["relations"]["changed_by"], "known");
    let change = &edges_of(&detail, "changed_by")[0];
    assert_eq!(
        change["source_ref"],
        format!("control.deployment_markers/{deploy}")
    );
    assert_eq!(change["confidence"], "recorded");
    assert_eq!(change["valid_from_basis"], "source");

    assert_eq!(
        w.get(&format!("/v1/guidance/graph?from={j}&depth=4"))
            .await
            .0,
        400
    );
    assert_eq!(
        w.get(&format!("/v1/guidance/graph?from={j}&relations=depends_on"))
            .await
            .0,
        400
    );
}

// OBS-39-02, OBS-39-10, OBS-39-03, OBS-39-12, OBS-39-13
#[tokio::test]
async fn ownership_at_incident_time_is_unchanged_by_later_changes() {
    let w = World::start().await;
    let (checkout, journey, first, second) = (
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
    );
    let t0 = Utc::now() - Duration::hours(6);
    let feed = |owners: Vec<FeedOwner>, journeys: Vec<FeedJourneyMember>| CatalogFeed {
        services: vec![service(checkout, "Checkout")],
        bindings: vec![binding(checkout, "checkout", "prod")],
        journeys: vec![FeedNamed {
            id: journey,
            name: "Buy".into(),
        }],
        journey_services: journeys,
        owners,
        ..Default::default()
    };
    w.catalog(feed(
        vec![owner(checkout, first, t0 - Duration::days(1))],
        vec![],
    ));
    w.sync(t0).await;
    let s = w.entity_id("service", checkout).await;

    // Before anything was recorded: say so, never fall back to current context.
    let (status, before) = w
        .get(&format!(
            "/v1/guidance/entities/{s}?at={}",
            at(t0 - Duration::minutes(5))
        ))
        .await;
    assert_eq!(status, 200, "{before}");
    assert_eq!(before["context"], "no_recorded_context");
    assert_eq!(before["relationships"], json!([]));

    let t1 = t0 + Duration::hours(1);
    let incident = w
        .incident("Checkout errors", t1, &["checkout"], "Dana")
        .await;
    w.sync(t1 + Duration::minutes(1)).await;
    let read_at = t1 + Duration::minutes(2);
    let entity_uri = format!("/v1/guidance/entities/{s}?at={}", at(read_at));
    let i = w.entity_id("incident", incident).await;
    let graph_uri = format!("/v1/guidance/graph?from={i}&depth=2&at={}", at(read_at));
    let (_, then_entity, _) = w.get_as(w.tenant, &entity_uri).await;
    let (_, then_graph, _) = w.get_as(w.tenant, &graph_uri).await;
    assert_eq!(then_entity["context"], "recorded", "{then_entity}");
    assert_eq!(then_entity["since_then"], json!([]));
    let owners: Vec<&Value> = edges_of(&then_entity, "owned_by");
    assert_eq!(owners.len(), 1);
    assert_eq!(
        owners[0]["entity"]["canonical_key"],
        format!("user:{first}")
    );
    // Topology from the incident: incident -> service -> owner.
    assert!(keys(&then_graph["nodes"]).contains(&("owner".into(), format!("user:{first}"))));

    // Ownership changes after the incident, and the journey is edited.
    let t2 = t0 + Duration::hours(3);
    w.catalog(feed(
        vec![owner(checkout, second, t2 - Duration::minutes(10))],
        vec![member(journey, checkout)],
    ));
    w.sync(t2).await;

    // The as-of read is what was recorded then, byte for byte.
    let (_, again_entity, _) = w.get_as(w.tenant, &entity_uri).await;
    let (_, again_graph, _) = w.get_as(w.tenant, &graph_uri).await;
    for field in [
        "entity",
        "context",
        "relationships",
        "relations",
        "unrecorded_sources",
    ] {
        assert_eq!(
            again_entity[field].to_string(),
            then_entity[field].to_string(),
            "{field}"
        );
    }
    for field in ["nodes", "edges"] {
        assert_eq!(
            again_graph[field].to_string(),
            then_graph[field].to_string(),
            "{field}"
        );
    }
    // What changed since is reported apart: the first owner's interval is now closed.
    let old = owners[0]["assertion_id"].clone();
    let closed = again_entity["since_then"].as_array().unwrap();
    assert_eq!(closed.len(), 1, "{again_entity}");
    assert_eq!(closed[0]["assertion_id"], old);
    assert_eq!(closed[0]["current"], false);
    assert!(closed[0]["valid_to"].is_string());

    // Now: the second owner is current and the expired first owner is not shown as current.
    let (_, current) = w.get(&format!("/v1/guidance/entities/{s}")).await;
    let owners = edges_of(&current, "owned_by");
    assert_eq!(owners.len(), 1, "{current}");
    assert_eq!(
        owners[0]["entity"]["canonical_key"],
        format!("user:{second}")
    );
    assert_eq!(owners[0]["current"], true);
    assert_eq!(owners[0]["valid_to"], Value::Null);
    assert!(
        current["relationships"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["assertion_id"] != old)
    );
    assert_eq!(edges_of(&current, "part_of_journey").len(), 1);
    assert!(edges_of(&then_entity, "part_of_journey").is_empty());
}

// OBS-39-06, OBS-39-09
#[tokio::test]
async fn sources_resolve_to_one_service_and_similar_names_stay_apart() {
    let w = World::start().await;
    let (checkout, payments) = (Uuid::new_v4(), Uuid::new_v4());
    w.catalog(CatalogFeed {
        services: vec![service(checkout, "Checkout"), service(payments, "Payments")],
        bindings: vec![
            binding(checkout, "checkout", "prod"),
            binding(payments, "payments", ""),
        ],
        owners: vec![owner(
            checkout,
            Uuid::new_v4(),
            Utc::now() - Duration::days(1),
        )],
        ..Default::default()
    });
    w.call_span(("checkout", "prod"), ("payments", "prod"), 600)
        .await;
    w.call_span(("checkout-v2", "prod"), ("payments", "prod"), 300)
        .await;
    w.sync(Utc::now()).await;

    // Catalog and telemetry name the same service: one entity carries both sources.
    let s = w.entity_id("service", checkout).await;
    let (status, detail) = w.get(&format!("/v1/guidance/entities/{s}")).await;
    assert_eq!(status, 200, "{detail}");
    let sources: HashSet<&str> = detail["relationships"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["source"].as_str().unwrap())
        .collect();
    assert_eq!(sources, HashSet::from(["catalog", "telemetry"]));
    let calls = edges_of(&detail, "calls");
    assert_eq!(calls.len(), 1, "{detail}");
    assert_eq!(calls[0]["entity"]["canonical_key"], payments.to_string());
    assert_eq!(calls[0]["confidence"], "inferred");
    assert_eq!(
        calls[0]["source_ref"],
        "telemetry.spans/checkout|prod>payments|prod"
    );
    assert_eq!(detail["relations"]["calls"], "known");

    // The similarly named, unbound name is its own unregistered service.
    let (status, v2) = w
        .get("/v1/guidance/entities?kind=unregistered_service&key=checkout-v2%7Cprod")
        .await;
    assert_eq!(status, 200, "{v2}");
    assert_ne!(v2["entity_id"], json!(s));
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alerting.guidance_entities WHERE tenant_id = $1 AND kind = 'service'",
    )
    .bind(w.tenant)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert_eq!(n, 2);

    // Every stored relationship names its source and the record it came from.
    let unsourced: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alerting.guidance_assertions \
         WHERE tenant_id = $1 AND (source = '' OR source_ref = '')",
    )
    .bind(w.tenant)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert_eq!(unsourced, 0);
}

// OBS-39-11
#[tokio::test]
async fn conflicting_owner_claims_are_both_shown_with_source_and_time() {
    let w = World::start().await;
    let (checkout, declared, directory) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let now = Utc::now();
    let conflict = Uuid::new_v4();
    w.catalog(CatalogFeed {
        services: vec![service(checkout, "Checkout")],
        bindings: vec![binding(checkout, "checkout", "prod")],
        owners: vec![owner(checkout, declared, now - Duration::days(3))],
        owner_conflicts: vec![FeedOwnerConflict {
            id: conflict,
            service_id: checkout,
            role: "operational".into(),
            catalog_principal_id: Some(declared),
            authoritative_principal_id: directory,
            detected_at: now - Duration::days(1),
        }],
        ..Default::default()
    });
    w.sync(now).await;
    w.sync(now + Duration::minutes(5)).await;

    let s = w.entity_id("service", checkout).await;
    let (_, detail) = w.get(&format!("/v1/guidance/entities/{s}")).await;
    let mut owners: Vec<(String, String, String, bool)> = edges_of(&detail, "owned_by")
        .iter()
        .map(|e| {
            (
                e["entity"]["canonical_key"].as_str().unwrap().into(),
                e["source"].as_str().unwrap().into(),
                e["valid_from"].as_str().unwrap().into(),
                e["conflict"].as_bool().unwrap(),
            )
        })
        .collect();
    owners.sort();
    assert_eq!(owners.len(), 2, "{detail}");
    assert_eq!(owners[0].0, format!("principal:{directory}"));
    assert_eq!(owners[0].1, "catalog_authoritative");
    assert_eq!(owners[1].0, format!("user:{declared}"));
    assert_eq!(owners[1].1, "catalog");
    assert!(owners.iter().all(|o| o.3), "both claims are flagged");
    assert!(owners.iter().all(|o| !o.2.is_empty()));

    // Re-syncing the same claims adds and replaces nothing.
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alerting.guidance_assertions WHERE tenant_id = $1",
    )
    .bind(w.tenant)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert_eq!(rows, 2);
}

// OBS-39-14
#[tokio::test]
async fn history_follows_the_requesters_current_access() {
    let w = World::start().await;
    let (checkout, ledger, journey) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let t0 = Utc::now() - Duration::hours(2);
    w.catalog(CatalogFeed {
        services: vec![service(checkout, "Checkout"), service(ledger, "Ledger")],
        bindings: vec![
            binding(checkout, "checkout", "prod"),
            binding(ledger, "ledger", "prod"),
        ],
        journeys: vec![FeedNamed {
            id: journey,
            name: "Buy".into(),
        }],
        journey_services: vec![member(journey, checkout), member(journey, ledger)],
        ..Default::default()
    });
    w.sync(t0).await;
    let incident = w
        .incident("Ledger lag", t0 + Duration::minutes(10), &["ledger"], "Kim")
        .await;
    w.sync(t0 + Duration::minutes(20)).await;
    w.hide(ledger);

    let hidden = w.entity_id("service", ledger).await;
    let hidden_incident = w.entity_id("incident", incident).await;
    let past = at(t0 + Duration::minutes(30));
    let (_, missing, missing_text) = w
        .get_as(
            w.tenant,
            &format!("/v1/guidance/entities/{}", Uuid::new_v4()),
        )
        .await;
    assert_eq!(missing, json!({"error": "not_found"}));
    for uri in [
        format!("/v1/guidance/entities/{hidden}"),
        format!("/v1/guidance/entities/{hidden}?at={past}"),
        format!("/v1/guidance/graph?from={hidden}"),
        format!("/v1/guidance/graph?from={hidden}&at={past}"),
        format!("/v1/guidance/entities/{hidden_incident}?at={past}"),
        format!("/v1/guidance/entities?kind=service&key={ledger}"),
    ] {
        let (status, _, text) = w.get_as(w.tenant, &uri).await;
        assert_eq!(
            (status, text.as_str()),
            (404, missing_text.as_str()),
            "{uri}"
        );
    }

    // A visible journey's history omits the hidden service, now and as of the past.
    let j = w.entity_id("journey", journey).await;
    for uri in [
        format!("/v1/guidance/graph?from={j}&depth=3"),
        format!("/v1/guidance/graph?from={j}&depth=3&at={past}"),
    ] {
        let (status, graph) = w.get(&uri).await;
        assert_eq!(status, 200, "{graph}");
        assert_eq!(
            keys(&graph["nodes"]),
            HashSet::from([
                ("journey".into(), journey.to_string()),
                ("service".into(), checkout.to_string()),
            ]),
            "{uri}"
        );
        assert_eq!(graph["edges"].as_array().unwrap().len(), 1);
    }

    // Another tenant sees nothing of this one.
    let s = w.entity_id("service", checkout).await;
    let (status, _, text) = w
        .get_as(Uuid::new_v4(), &format!("/v1/guidance/entities/{s}"))
        .await;
    assert_eq!((status, text.as_str()), (404, missing_text.as_str()));
}

// OBS-39-13 (negative), OBS-39-09 (negative)
#[tokio::test]
async fn the_ledger_refuses_rewrites_and_relationships_without_a_source() {
    let w = World::start().await;
    let (checkout, first) = (Uuid::new_v4(), Uuid::new_v4());
    let t0 = Utc::now() - Duration::hours(1);
    let feed = |principal: Uuid| CatalogFeed {
        services: vec![service(checkout, "Checkout")],
        owners: vec![owner(checkout, principal, t0 - Duration::days(1))],
        ..Default::default()
    };
    w.catalog(feed(first));
    w.sync(t0).await;
    w.catalog(feed(Uuid::new_v4()));
    w.sync(t0 + Duration::minutes(10)).await;

    // Each relationship detail names the source and the source record that asserted it.
    let s = w.entity_id("service", checkout).await;
    let (_, detail) = w.get(&format!("/v1/guidance/entities/{s}")).await;
    let owners = edges_of(&detail, "owned_by");
    assert_eq!(owners.len(), 1, "{detail}");
    assert_eq!(owners[0]["source"], "catalog");
    assert_eq!(
        owners[0]["source_ref"],
        format!("catalog.service_owners/{checkout}/operational")
    );
    assert_eq!(owners[0]["confidence"], "declared");

    let closed: Uuid = sqlx::query_scalar(
        "SELECT assertion_id FROM alerting.guidance_assertions \
         WHERE tenant_id = $1 AND valid_to IS NOT NULL",
    )
    .bind(w.tenant)
    .fetch_one(&w.state.db)
    .await
    .unwrap();
    // As the alerting login: closed history cannot be reopened, moved or edited.
    for sql in [
        "UPDATE alerting.guidance_assertions SET valid_to = NULL WHERE assertion_id = $1",
        "UPDATE alerting.guidance_assertions SET valid_to = valid_to + INTERVAL '1 day' WHERE assertion_id = $1",
        "UPDATE alerting.guidance_assertions SET source_ref = 'rewritten' WHERE assertion_id = $1",
    ] {
        let err = sqlx::query(sql)
            .bind(closed)
            .execute(&w.state.db)
            .await
            .expect_err(sql);
        assert!(err.to_string().contains("append-only"), "{err}");
    }
    let open: Uuid = sqlx::query_scalar(
        "SELECT assertion_id FROM alerting.guidance_assertions \
         WHERE tenant_id = $1 AND valid_to IS NULL",
    )
    .bind(w.tenant)
    .fetch_one(&w.state.db)
    .await
    .unwrap();
    assert!(
        sqlx::query("UPDATE alerting.guidance_assertions SET confidence = 'inferred' WHERE assertion_id = $1")
            .bind(open)
            .execute(&w.state.db)
            .await
            .is_err()
    );

    // A relationship without a source or source record is never stored.
    for (source, source_ref) in [("", "catalog.x/1"), ("rumour", "x"), ("catalog", "")] {
        let res = sqlx::query(
            "INSERT INTO alerting.guidance_assertions \
             (tenant_id, subject_id, relation, object_id, source, source_ref, confidence, \
              valid_from, valid_from_basis) \
             SELECT tenant_id, subject_id, relation, object_id, $2, $3, confidence, now(), 'source' \
             FROM alerting.guidance_assertions WHERE assertion_id = $1",
        )
        .bind(open)
        .bind(source)
        .bind(source_ref)
        .execute(&w.state.db)
        .await;
        assert!(res.is_err(), "{source:?} {source_ref:?}");
    }

    // A failed catalog pull records an incomplete run and ends nothing.
    w.feeds.lock().unwrap().catalog_unavailable = true;
    w.sync(t0 + Duration::minutes(20)).await;
    let still_open: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM alerting.guidance_assertions WHERE tenant_id = $1 AND valid_to IS NULL",
    )
    .bind(w.tenant)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert_eq!(still_open, 1);
    let failed: Option<String> = sqlx::query_scalar(
        "SELECT error FROM alerting.guidance_source_runs \
         WHERE tenant_id = $1 AND source = 'catalog' AND NOT complete",
    )
    .bind(w.tenant)
    .fetch_one(&w.owner)
    .await
    .unwrap();
    assert!(failed.is_some());
}
