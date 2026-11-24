//! Operational guidance graph (`manage-operational-guidance`, W4a of
//! docs/operational-guidance-graph.md, OBS-39-01..03 and 06..14).
//!
//! An append-only ledger of relationships between stable entities (services, owners,
//! journeys, providers, changes, incidents). Each relationship carries its source,
//! confidence and validity interval. A worker pulls the catalog's current relationships,
//! control's deployments, this component's incidents and telemetry calls, and records each
//! change: a new claim opens an interval, a vanished one closes it. Nothing is overwritten,
//! so a read "as of" an earlier time sees what was recorded then.
//!
//! Every read authorizes services, journeys and providers against the catalog with the
//! caller's credential. A hidden entity is dropped with every edge to it and is the same
//! 404 as a missing one. Changes and incidents follow their service.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;
use crate::middleware::auth::TenantContext;

pub const KINDS: &[&str] = &[
    "service",
    "unregistered_service",
    "owner",
    "journey",
    "provider",
    "change",
    "incident",
];
pub const RELATIONS: &[&str] = &[
    "owned_by",
    "part_of_journey",
    "provided_by",
    "changed_by",
    "affected",
    "calls",
];
/// Relations a service is expected to have; each is reported `known` or `unknown`.
const SERVICE_RELATIONS: &[&str] = &[
    "owned_by",
    "part_of_journey",
    "provided_by",
    "changed_by",
    "affected",
    "calls",
];
const TELEMETRY_LOOKBACK_HOURS: i64 = 24;
const DEPLOYMENT_FIRST_LOOKBACK_DAYS: i64 = 30;
const MAX_GRAPH_NODES: usize = 200;

// --- Sources ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CatalogFeed {
    pub services: Vec<FeedService>,
    pub bindings: Vec<FeedBinding>,
    pub journeys: Vec<FeedNamed>,
    pub journey_services: Vec<FeedJourneyMember>,
    pub owners: Vec<FeedOwner>,
    pub providers: Vec<FeedNamed>,
    pub provider_dependencies: Vec<FeedProviderDependency>,
    pub owner_conflicts: Vec<FeedOwnerConflict>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedService {
    pub id: Uuid,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedBinding {
    pub service_id: Uuid,
    pub service_name: String,
    pub environment: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedNamed {
    pub id: Uuid,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedJourneyMember {
    pub journey_id: Uuid,
    pub service_id: Uuid,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedOwner {
    pub service_id: Uuid,
    pub role: String,
    pub principal_kind: String,
    pub principal_id: Uuid,
    pub assigned_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedProviderDependency {
    pub id: Uuid,
    pub provider_id: Option<Uuid>,
    pub provider_name: String,
    pub service_id: Option<Uuid>,
    pub journey_id: Option<Uuid>,
    pub function: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedOwnerConflict {
    pub id: Uuid,
    pub service_id: Uuid,
    pub role: String,
    pub catalog_principal_id: Option<Uuid>,
    pub authoritative_principal_id: Uuid,
    pub detected_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FeedDeployment {
    pub deployment_id: Uuid,
    pub service_name: String,
    pub environment: String,
    pub service_version: String,
    pub status: String,
    pub started_at: DateTime<Utc>,
}

#[derive(Default)]
pub struct StaticSources {
    pub catalog: HashMap<Uuid, CatalogFeed>,
    pub deployments: HashMap<Uuid, Vec<FeedDeployment>>,
    pub catalog_unavailable: bool,
    /// Recorded observability cost per (tenant, period), as control reports it.
    pub costs: HashMap<(Uuid, String), i64>,
}

/// Where the ledger's external relationships come from.
#[derive(Clone)]
pub enum GuidanceSources {
    Disabled,
    Static(Arc<Mutex<StaticSources>>),
    Http {
        catalog_url: String,
        control_url: String,
        token: String,
        client: reqwest::Client,
    },
}

impl GuidanceSources {
    pub fn substitute() -> (Self, Arc<Mutex<StaticSources>>) {
        let s = Arc::new(Mutex::new(StaticSources::default()));
        (Self::Static(s.clone()), s)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        client: &reqwest::Client,
        url: String,
        token: &str,
    ) -> anyhow::Result<T> {
        let resp = client
            .get(url)
            .header("X-Internal-Token", token)
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("source returned {}", resp.status());
        }
        Ok(resp.json().await?)
    }

    pub async fn catalog(&self, tenant: Uuid) -> anyhow::Result<CatalogFeed> {
        match self {
            Self::Disabled => anyhow::bail!("guidance sources disabled"),
            Self::Static(s) => {
                let s = s.lock().expect("substitute poisoned");
                if s.catalog_unavailable {
                    anyhow::bail!("catalog unavailable");
                }
                Ok(s.catalog.get(&tenant).cloned().unwrap_or_default())
            }
            Self::Http {
                catalog_url,
                token,
                client,
                ..
            } => {
                Self::get_json(
                    client,
                    format!("{catalog_url}/internal/catalog/relationships?tenant_id={tenant}"),
                    token,
                )
                .await
            }
        }
    }

    /// Control's recorded observability cost for a period (`YYYY-MM`); none when nothing was
    /// recorded for it.
    pub async fn cost_total(&self, tenant: Uuid, period: &str) -> anyhow::Result<Option<i64>> {
        #[derive(Deserialize)]
        struct Body {
            total_cents: Option<i64>,
        }
        match self {
            Self::Disabled => anyhow::bail!("control source disabled"),
            Self::Static(s) => Ok(s
                .lock()
                .expect("substitute poisoned")
                .costs
                .get(&(tenant, period.to_owned()))
                .copied()),
            Self::Http {
                control_url,
                token,
                client,
                ..
            } => {
                let b: Body = Self::get_json(
                    client,
                    format!("{control_url}/internal/cost/totals?tenant_id={tenant}&period={period}"),
                    token,
                )
                .await?;
                Ok(b.total_cents)
            }
        }
    }

    pub async fn deployments(
        &self,
        tenant: Uuid,
        since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<FeedDeployment>> {
        #[derive(Deserialize)]
        struct Body {
            deployments: Vec<FeedDeployment>,
        }
        match self {
            Self::Disabled => anyhow::bail!("guidance sources disabled"),
            Self::Static(s) => Ok(s
                .lock()
                .expect("substitute poisoned")
                .deployments
                .get(&tenant)
                .map(|d| {
                    d.iter()
                        .filter(|x| x.started_at >= since)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()),
            Self::Http {
                control_url,
                token,
                client,
                ..
            } => {
                let since = since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
                let b: Body = Self::get_json(
                    client,
                    format!("{control_url}/internal/deployments?tenant_id={tenant}&since={since}"),
                    token,
                )
                .await?;
                Ok(b.deployments)
            }
        }
    }
}

// --- Ingestion --------------------------------------------------------------------------------

/// One claim a source makes now.
#[derive(Clone, Debug)]
struct Claim {
    subject: (String, String, String),
    relation: &'static str,
    object: (String, String, String),
    qualifier: String,
    source: &'static str,
    source_ref: String,
    confidence: &'static str,
    valid_from: Option<DateTime<Utc>>,
    evidence: Value,
}

fn ent(kind: &str, key: impl Into<String>, name: impl Into<String>) -> (String, String, String) {
    (kind.to_owned(), key.into(), name.into())
}

/// Resolves a telemetry identity to the catalog service it is bound to, else an
/// `unregistered_service` of its own. Never by name similarity (OBS-39-06).
struct Resolver {
    exact: HashMap<(String, String), (Uuid, String)>,
}

impl Resolver {
    fn new(feed: &CatalogFeed) -> Self {
        let names: HashMap<Uuid, &str> = feed
            .services
            .iter()
            .map(|s| (s.id, s.name.as_str()))
            .collect();
        let exact = feed
            .bindings
            .iter()
            .filter_map(|b| {
                let name = names.get(&b.service_id)?;
                Some((
                    (b.service_name.clone(), b.environment.clone()),
                    (b.service_id, (*name).to_owned()),
                ))
            })
            .collect();
        Self { exact }
    }

    fn service(&self, name: &str, env: &str) -> (String, String, String) {
        let hit = self
            .exact
            .get(&(name.to_owned(), env.to_owned()))
            .or_else(|| self.exact.get(&(name.to_owned(), String::new())));
        match hit {
            Some((id, display)) => ent("service", id.to_string(), display.clone()),
            None => ent(
                "unregistered_service",
                format!("{name}|{env}"),
                format!("{name} ({env})"),
            ),
        }
    }

    /// Incidents name services without an environment: a name bound in exactly one service.
    fn by_name(&self, name: &str) -> (String, String, String) {
        let hits: BTreeSet<(Uuid, String)> = self
            .exact
            .iter()
            .filter(|((n, _), _)| n == name)
            .map(|(_, v)| v.clone())
            .collect();
        match hits.len() {
            1 => {
                let (id, display) = hits.into_iter().next().expect("one");
                ent("service", id.to_string(), display)
            }
            _ => ent("unregistered_service", format!("{name}|"), name.to_owned()),
        }
    }
}

fn catalog_claims(feed: &CatalogFeed) -> Vec<Claim> {
    let svc: HashMap<Uuid, &str> = feed
        .services
        .iter()
        .map(|s| (s.id, s.name.as_str()))
        .collect();
    let journeys: HashMap<Uuid, &str> = feed
        .journeys
        .iter()
        .map(|j| (j.id, j.name.as_str()))
        .collect();
    let providers: HashMap<Uuid, &str> = feed
        .providers
        .iter()
        .map(|p| (p.id, p.name.as_str()))
        .collect();
    let service = |id: Uuid| {
        ent(
            "service",
            id.to_string(),
            svc.get(&id).copied().unwrap_or(""),
        )
    };
    let mut out = Vec::new();
    for o in &feed.owners {
        out.push(Claim {
            subject: service(o.service_id),
            relation: "owned_by",
            object: ent(
                "owner",
                format!("{}:{}", o.principal_kind, o.principal_id),
                format!("{}:{}", o.principal_kind, o.principal_id),
            ),
            qualifier: o.role.clone(),
            source: "catalog",
            source_ref: format!("catalog.service_owners/{}/{}", o.service_id, o.role),
            confidence: "declared",
            valid_from: Some(o.assigned_at),
            evidence: json!({}),
        });
    }
    for c in &feed.owner_conflicts {
        out.push(Claim {
            subject: service(c.service_id),
            relation: "owned_by",
            object: ent(
                "owner",
                format!("principal:{}", c.authoritative_principal_id),
                format!("principal:{}", c.authoritative_principal_id),
            ),
            qualifier: c.role.clone(),
            source: "catalog_authoritative",
            source_ref: format!("catalog.authoritative_sync_conflicts/{}", c.id),
            confidence: "declared",
            valid_from: Some(c.detected_at),
            evidence: json!({}),
        });
    }
    for m in &feed.journey_services {
        out.push(Claim {
            subject: service(m.service_id),
            relation: "part_of_journey",
            object: ent(
                "journey",
                m.journey_id.to_string(),
                journeys.get(&m.journey_id).copied().unwrap_or(""),
            ),
            qualifier: String::new(),
            source: "catalog",
            source_ref: format!("catalog.journey_services/{}/{}", m.journey_id, m.service_id),
            confidence: "declared",
            valid_from: None,
            evidence: json!({}),
        });
    }
    for d in &feed.provider_dependencies {
        let subject = match (d.service_id, d.journey_id) {
            (Some(s), _) => service(s),
            (None, Some(j)) => ent(
                "journey",
                j.to_string(),
                journeys.get(&j).copied().unwrap_or(""),
            ),
            (None, None) => continue,
        };
        let object = match d.provider_id {
            Some(p) => ent(
                "provider",
                p.to_string(),
                providers.get(&p).copied().unwrap_or(&d.provider_name),
            ),
            None => ent(
                "provider",
                format!("name:{}", d.provider_name.to_lowercase()),
                d.provider_name.clone(),
            ),
        };
        out.push(Claim {
            subject,
            relation: "provided_by",
            object,
            qualifier: d.function.clone(),
            source: "catalog",
            source_ref: format!("catalog.provider_dependencies/{}", d.id),
            confidence: "declared",
            valid_from: Some(d.created_at),
            evidence: json!({}),
        });
    }
    out
}

async fn entity_id(db: &PgPool, tenant: Uuid, e: &(String, String, String)) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO alerting.guidance_entities (tenant_id, kind, canonical_key, display_name) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (tenant_id, kind, canonical_key) DO UPDATE SET display_name = \
           CASE WHEN EXCLUDED.display_name <> '' THEN EXCLUDED.display_name \
                ELSE alerting.guidance_entities.display_name END \
         RETURNING entity_id",
    )
    .bind(tenant)
    .bind(&e.0)
    .bind(&e.1)
    .bind(&e.2)
    .fetch_one(db)
    .await
}

#[derive(sqlx::FromRow)]
struct OpenRow {
    assertion_id: Uuid,
    subject_id: Uuid,
    relation: String,
    object_id: Uuid,
    qualifier: String,
    source: String,
    valid_from: DateTime<Utc>,
}

type ClaimKey = (Uuid, String, Uuid, String, String);

/// Which open claims of the source a sync may close when it no longer makes them.
#[derive(Clone, Copy, PartialEq)]
enum Close {
    /// The source reports its full current state.
    All,
    /// The source reports full state only for the subjects it returned (a time window).
    ReturnedSubjects,
    /// Events: once recorded, never closed by absence.
    Never,
}

/// Records the claims of `sources` as they stand now: opens new claims, closes vanished ones.
/// A vanished claim closes at this sync, never earlier, so a read as of any past time keeps
/// returning what was recorded then (OBS-39-13).
async fn apply_claims(
    db: &PgPool,
    tenant: Uuid,
    sources: &[&str],
    claims: Vec<Claim>,
    now: DateTime<Utc>,
    close: Close,
) -> sqlx::Result<()> {
    let mut wanted: HashMap<ClaimKey, Claim> = HashMap::new();
    for c in claims {
        let s = entity_id(db, tenant, &c.subject).await?;
        let o = entity_id(db, tenant, &c.object).await?;
        wanted.insert(
            (
                s,
                c.relation.to_owned(),
                o,
                c.qualifier.clone(),
                c.source.to_owned(),
            ),
            c,
        );
    }
    let sources: Vec<String> = sources.iter().map(|s| (*s).to_owned()).collect();
    let open: Vec<OpenRow> = sqlx::query_as(
        "SELECT assertion_id, subject_id, relation, object_id, qualifier, source, valid_from \
         FROM alerting.guidance_assertions \
         WHERE tenant_id = $1 AND source = ANY($2) AND valid_to IS NULL",
    )
    .bind(tenant)
    .bind(&sources)
    .fetch_all(db)
    .await?;
    let open_keys: HashSet<ClaimKey> = open
        .iter()
        .map(|r| {
            (
                r.subject_id,
                r.relation.clone(),
                r.object_id,
                r.qualifier.clone(),
                r.source.clone(),
            )
        })
        .collect();
    let returned: HashSet<Uuid> = wanted.keys().map(|k| k.0).collect();
    if close != Close::Never {
        for r in &open {
            if close == Close::ReturnedSubjects && !returned.contains(&r.subject_id) {
                continue;
            }
            let key = (
                r.subject_id,
                r.relation.clone(),
                r.object_id,
                r.qualifier.clone(),
                r.source.clone(),
            );
            if wanted.contains_key(&key) {
                continue;
            }
            sqlx::query(
                "UPDATE alerting.guidance_assertions SET valid_to = $3 \
                 WHERE tenant_id = $1 AND assertion_id = $2 AND valid_to IS NULL",
            )
            .bind(tenant)
            .bind(r.assertion_id)
            .bind(now.max(r.valid_from))
            .execute(db)
            .await?;
        }
    }
    for (key, c) in wanted {
        if open_keys.contains(&key) {
            continue;
        }
        let (from, basis) = match c.valid_from {
            Some(t) if t <= now => (t, "source"),
            _ => (now, "observed_at_sync"),
        };
        let fingerprint = c.evidence["fingerprint"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        sqlx::query(
            "INSERT INTO alerting.guidance_assertions \
             (tenant_id, subject_id, relation, object_id, qualifier, source, source_ref, \
              confidence, valid_from, valid_from_basis, recorded_at, evidence, evidence_fingerprint) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (tenant_id, subject_id, relation, object_id, qualifier, source) \
               WHERE valid_to IS NULL DO NOTHING",
        )
        .bind(tenant)
        .bind(key.0)
        .bind(c.relation)
        .bind(key.2)
        .bind(&c.qualifier)
        .bind(c.source)
        .bind(&c.source_ref)
        .bind(c.confidence)
        .bind(from)
        .bind(basis)
        .bind(now)
        .bind(&c.evidence)
        .bind(fingerprint)
        .execute(db)
        .await?;
    }
    Ok(())
}

async fn start_run(
    db: &PgPool,
    tenant: Uuid,
    source: &str,
    now: DateTime<Utc>,
) -> sqlx::Result<Uuid> {
    sqlx::query_scalar(
        "INSERT INTO alerting.guidance_source_runs (tenant_id, source, started_at) \
         VALUES ($1, $2, $3) RETURNING run_id",
    )
    .bind(tenant)
    .bind(source)
    .bind(now)
    .fetch_one(db)
    .await
}

async fn finish_run(db: &PgPool, run: Uuid, error: Option<String>) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE alerting.guidance_source_runs SET completed_at = now(), complete = $2, error = $3 \
         WHERE run_id = $1",
    )
    .bind(run)
    .bind(error.is_none())
    .bind(error)
    .execute(db)
    .await?;
    Ok(())
}

#[derive(clickhouse::Row, Deserialize)]
struct CallRow {
    caller: String,
    caller_env: String,
    callee: String,
    callee_env: String,
    calls: u64,
    first_ns: u64,
    last_ns: u64,
    trace_ids: Vec<String>,
}

async fn telemetry_calls(
    ch: &clickhouse::Client,
    tenant: Uuid,
    since: DateTime<Utc>,
) -> Result<Vec<CallRow>, clickhouse::error::Error> {
    let since_ns = since.timestamp_nanos_opt().unwrap_or(0).max(0) as u64;
    ch.query(
        "SELECT p.service_name AS caller, p.environment AS caller_env, \
                c.service_name AS callee, c.environment AS callee_env, \
                count() AS calls, min(c.start_time_unix_nano) AS first_ns, \
                max(c.start_time_unix_nano) AS last_ns, \
                groupUniqArray(5)(c.trace_id) AS trace_ids \
         FROM observable.spans AS c \
         INNER JOIN (SELECT trace_id, span_id, service_name, environment FROM observable.spans \
                     WHERE tenant_id = ? AND start_time_unix_nano >= ?) AS p \
           ON c.trace_id = p.trace_id AND c.parent_span_id = p.span_id \
         WHERE c.tenant_id = ? AND c.start_time_unix_nano >= ? \
           AND c.service_name != p.service_name \
         GROUP BY caller, caller_env, callee, callee_env \
         ORDER BY calls DESC LIMIT 2000",
    )
    .bind(tenant)
    .bind(since_ns)
    .bind(tenant)
    .bind(since_ns)
    .fetch_all()
    .await
}

fn ns_time(ns: u64) -> Option<DateTime<Utc>> {
    i64::try_from(ns).ok().map(DateTime::from_timestamp_nanos)
}

/// Pulls every source once for `tenant`. A failed source records an incomplete run and
/// closes nothing, so an outage never reads as relationships ending.
pub async fn sync_tenant(
    db: &PgPool,
    ch: &clickhouse::Client,
    sources: &GuidanceSources,
    tenant: Uuid,
) -> anyhow::Result<()> {
    sync_tenant_at(db, ch, sources, tenant, Utc::now()).await
}

/// [`sync_tenant`] with the sync's clock given: what this sync records, it records at `now`.
pub async fn sync_tenant_at(
    db: &PgPool,
    ch: &clickhouse::Client,
    sources: &GuidanceSources,
    tenant: Uuid,
    now: DateTime<Utc>,
) -> anyhow::Result<()> {
    let run = start_run(db, tenant, "catalog", now).await?;
    let feed = match sources.catalog(tenant).await {
        Ok(f) => {
            apply_claims(
                db,
                tenant,
                &["catalog", "catalog_authoritative"],
                catalog_claims(&f),
                now,
                Close::All,
            )
            .await?;
            finish_run(db, run, None).await?;
            Some(f)
        }
        Err(e) => {
            finish_run(db, run, Some(e.to_string())).await?;
            None
        }
    };
    // Without the catalog's bindings no telemetry or deployment name can be resolved.
    let Some(feed) = feed else {
        return Ok(());
    };
    let resolver = Resolver::new(&feed);

    let run = start_run(db, tenant, "control", now).await?;
    let since: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT max(started_at) FROM alerting.guidance_source_runs \
         WHERE tenant_id = $1 AND source = 'control' AND complete",
    )
    .bind(tenant)
    .fetch_one(db)
    .await?;
    let since = since.map_or(
        now - chrono::Duration::days(DEPLOYMENT_FIRST_LOOKBACK_DAYS),
        |t| t - chrono::Duration::hours(1),
    );
    match sources.deployments(tenant, since).await {
        Ok(deps) => {
            let claims = deps
                .iter()
                .map(|d| Claim {
                    subject: resolver.service(&d.service_name, &d.environment),
                    relation: "changed_by",
                    object: ent(
                        "change",
                        d.deployment_id.to_string(),
                        format!(
                            "{} {} ({})",
                            d.service_name, d.service_version, d.environment
                        ),
                    ),
                    qualifier: String::new(),
                    source: "control",
                    source_ref: format!("control.deployment_markers/{}", d.deployment_id),
                    confidence: "recorded",
                    valid_from: Some(d.started_at),
                    evidence: json!({"deployment_id": d.deployment_id, "status": d.status,
                                     "service_version": d.service_version}),
                })
                .collect();
            // A change is an event; it stays recorded.
            apply_claims(db, tenant, &["control"], claims, now, Close::Never).await?;
            finish_run(db, run, None).await?;
        }
        Err(e) => finish_run(db, run, Some(e.to_string())).await?,
    }

    let run = start_run(db, tenant, "alerting", now).await?;
    #[derive(sqlx::FromRow)]
    struct IncidentRow {
        incident_id: Uuid,
        title: String,
        triggered_at: DateTime<Utc>,
        resolved_at: Option<DateTime<Utc>>,
        affected_services: Vec<String>,
        owner: Option<String>,
    }
    let incidents: Vec<IncidentRow> = sqlx::query_as(
        "SELECT incident_id, title, triggered_at, resolved_at, affected_services, owner \
         FROM alerting.incidents \
         WHERE tenant_id = $1 AND triggered_at <= $2 AND triggered_at >= $2 - INTERVAL '90 days'",
    )
    .bind(tenant)
    .bind(now)
    .fetch_all(db)
    .await?;
    // A resolved incident whose effect is already closed is not asserted again.
    let closed: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT e.canonical_key FROM alerting.guidance_assertions a \
         JOIN alerting.guidance_entities e ON e.entity_id = a.subject_id \
         WHERE a.tenant_id = $1 AND e.kind = 'incident' AND a.source = 'alerting' \
           AND a.valid_to IS NOT NULL",
    )
    .bind(tenant)
    .fetch_all(db)
    .await?
    .into_iter()
    .collect();
    let mut claims = Vec::new();
    for i in incidents.iter().filter(|i| {
        !(i.resolved_at.is_some_and(|r| r <= now) && closed.contains(&i.incident_id.to_string()))
    }) {
        // The incident's recorded owner is a name, not a catalog principal.
        if let Some(owner) = i.owner.as_deref().map(str::trim).filter(|o| !o.is_empty()) {
            claims.push(Claim {
                subject: ent("incident", i.incident_id.to_string(), i.title.clone()),
                relation: "owned_by",
                object: ent("owner", format!("name:{}", owner.to_lowercase()), owner),
                qualifier: "incident".into(),
                source: "alerting",
                source_ref: format!("alerting.incidents/{}/owner", i.incident_id),
                confidence: "recorded",
                valid_from: Some(i.triggered_at),
                evidence: json!({"incident_id": i.incident_id}),
            });
        }
        for name in &i.affected_services {
            claims.push(Claim {
                subject: ent("incident", i.incident_id.to_string(), i.title.clone()),
                relation: "affected",
                object: resolver.by_name(name),
                qualifier: String::new(),
                source: "alerting",
                source_ref: format!("alerting.incidents/{}", i.incident_id),
                confidence: "recorded",
                valid_from: Some(i.triggered_at),
                evidence: json!({"incident_id": i.incident_id}),
            });
        }
    }
    apply_claims(
        db,
        tenant,
        &["alerting"],
        claims,
        now,
        Close::ReturnedSubjects,
    )
    .await?;
    // A resolved incident's effect is closed by the first sync that sees it resolved; the
    // resolution time itself stays in the incident record.
    for i in incidents
        .iter()
        .filter(|i| i.resolved_at.is_some_and(|r| r <= now))
    {
        sqlx::query(
            "UPDATE alerting.guidance_assertions a SET valid_to = GREATEST($3, a.valid_from) \
             FROM alerting.guidance_entities e \
             WHERE a.tenant_id = $1 AND a.subject_id = e.entity_id AND e.kind = 'incident' \
               AND e.canonical_key = $2 AND a.source = 'alerting' AND a.valid_to IS NULL",
        )
        .bind(tenant)
        .bind(i.incident_id.to_string())
        .bind(now)
        .execute(db)
        .await?;
    }
    finish_run(db, run, None).await?;

    let run = start_run(db, tenant, "telemetry", now).await?;
    match telemetry_calls(
        ch,
        tenant,
        now - chrono::Duration::hours(TELEMETRY_LOOKBACK_HOURS),
    )
    .await
    {
        Ok(rows) => {
            let claims: Vec<Claim> = rows
                .iter()
                .map(|r| {
                    let caller = resolver.service(&r.caller, &r.caller_env);
                    let callee = resolver.service(&r.callee, &r.callee_env);
                    let mut traces = r.trace_ids.clone();
                    traces.sort();
                    let fingerprint = format!("{}>{}@{}", caller.1, callee.1, r.last_ns);
                    Claim {
                        source_ref: format!(
                            "telemetry.spans/{}|{}>{}|{}",
                            r.caller, r.caller_env, r.callee, r.callee_env
                        ),
                        subject: caller,
                        relation: "calls",
                        object: callee,
                        qualifier: String::new(),
                        source: "telemetry",
                        confidence: "inferred",
                        valid_from: ns_time(r.first_ns),
                        evidence: json!({
                            "calls": r.calls,
                            "first_seen": ns_time(r.first_ns),
                            "last_seen": ns_time(r.last_ns),
                            "lookback_hours": TELEMETRY_LOOKBACK_HOURS,
                            "trace_ids": traces,
                            // Same calls re-read give the same fingerprint; calls seen later
                            // give a new one (OBS-39-20).
                            "fingerprint": fingerprint,
                        }),
                    }
                })
                .collect();
            let latest: Vec<(String, String, Value)> = claims
                .iter()
                .map(|c| (c.subject.1.clone(), c.object.1.clone(), c.evidence.clone()))
                .collect();
            apply_claims(db, tenant, &["telemetry"], claims, now, Close::All).await?;
            record_inference_state(db, tenant, &latest).await?;
            finish_run(db, run, None).await?;
        }
        Err(e) => finish_run(db, run, Some(e.to_string())).await?,
    }
    Ok(())
}

/// Keeps the latest evidence of each open inferred `calls` assertion, which review state
/// compares against. The assertion row itself never changes.
async fn record_inference_state(
    db: &PgPool,
    tenant: Uuid,
    latest: &[(String, String, Value)],
) -> sqlx::Result<()> {
    for (subject_key, object_key, evidence) in latest {
        let Some(last_seen) = evidence["last_seen"]
            .as_str()
            .and_then(|t| t.parse::<DateTime<Utc>>().ok())
        else {
            continue;
        };
        sqlx::query(
            "INSERT INTO alerting.guidance_inference_state \
               (assertion_id, tenant_id, evidence_fingerprint, last_seen, evidence) \
             SELECT a.assertion_id, a.tenant_id, $4, $5, $6 \
             FROM alerting.guidance_assertions a \
             JOIN alerting.guidance_entities s ON s.entity_id = a.subject_id \
             JOIN alerting.guidance_entities o ON o.entity_id = a.object_id \
             WHERE a.tenant_id = $1 AND a.source = 'telemetry' AND a.relation = 'calls' \
               AND a.valid_to IS NULL AND s.canonical_key = $2 AND o.canonical_key = $3 \
             ON CONFLICT (assertion_id) DO UPDATE SET \
               evidence_fingerprint = EXCLUDED.evidence_fingerprint, \
               last_seen = EXCLUDED.last_seen, evidence = EXCLUDED.evidence, updated_at = now()",
        )
        .bind(tenant)
        .bind(subject_key)
        .bind(object_key)
        .bind(evidence["fingerprint"].as_str().unwrap_or_default())
        .bind(last_seen)
        .bind(evidence)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// Syncs every tenant this component knows, every `interval`.
pub async fn start_sync_worker(
    db: PgPool,
    ch: clickhouse::Client,
    sources: GuidanceSources,
    interval: Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;
        let tenants: Vec<Uuid> = match sqlx::query_scalar(
            "SELECT tenant_id FROM alert_rules UNION SELECT tenant_id FROM incidents \
             UNION SELECT tenant_id FROM alerting.service_objectives",
        )
        .fetch_all(&db)
        .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "guidance sync: tenant list failed");
                continue;
            }
        };
        for tenant in tenants {
            if let Err(e) = sync_tenant(&db, &ch, &sources, tenant).await {
                tracing::warn!(tenant_id = %tenant, error = %e, "guidance sync failed");
            }
        }
    }
}

// --- Access -----------------------------------------------------------------------------------

/// Read authorization of catalog-owned entities, with the caller's credential.
#[derive(Clone)]
pub enum GuidanceAccess {
    Disabled,
    Static(Arc<Mutex<HashSet<Uuid>>>),
    Http {
        base_url: Arc<String>,
        client: reqwest::Client,
    },
}

#[derive(Debug)]
pub enum AccessError {
    Unauthorized,
    Unavailable,
}

impl GuidanceAccess {
    /// Static: every id is visible except the hidden ones in the returned set.
    pub fn substitute() -> (Self, Arc<Mutex<HashSet<Uuid>>>) {
        let s = Arc::new(Mutex::new(HashSet::new()));
        (Self::Static(s.clone()), s)
    }

    async fn visible(
        &self,
        headers: &HeaderMap,
        kind: &str,
        id: Uuid,
    ) -> Result<bool, AccessError> {
        match self {
            Self::Disabled => Err(AccessError::Unavailable),
            Self::Static(hidden) => Ok(!hidden.lock().expect("substitute poisoned").contains(&id)),
            Self::Http { base_url, client } => {
                let path = match kind {
                    "service" => "services",
                    "journey" => "journeys",
                    "provider" => "providers",
                    _ => return Ok(true),
                };
                let mut req = client
                    .get(format!(
                        "{}/v1/catalog/{path}/{id}",
                        base_url.trim_end_matches('/')
                    ))
                    .timeout(Duration::from_secs(5));
                for h in [header::AUTHORIZATION, header::COOKIE] {
                    if let Some(v) = headers.get(&h) {
                        req = req.header(h, v);
                    }
                }
                match req.send().await {
                    Ok(r) if r.status().is_success() => Ok(true),
                    Ok(r) if matches!(r.status().as_u16(), 403 | 404) => Ok(false),
                    Ok(r) if r.status().as_u16() == 401 => Err(AccessError::Unauthorized),
                    _ => Err(AccessError::Unavailable),
                }
            }
        }
    }
}

// --- Reads ------------------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, sqlx::FromRow)]
pub struct Entity {
    pub entity_id: Uuid,
    pub kind: String,
    pub canonical_key: String,
    pub display_name: String,
}

#[derive(Clone, Debug, sqlx::FromRow)]
struct EdgeRow {
    assertion_id: Uuid,
    subject_id: Uuid,
    relation: String,
    object_id: Uuid,
    qualifier: String,
    source: String,
    source_ref: String,
    confidence: String,
    valid_from: DateTime<Utc>,
    valid_from_basis: String,
    valid_to: Option<DateTime<Utc>>,
    recorded_at: DateTime<Utc>,
    evidence: Value,
    evidence_fingerprint: String,
}

/// Review state of an edge. Only `Established` edges are returned as relationships.
#[derive(Clone, Debug, PartialEq)]
enum Review {
    /// Not inferred, or inferred and never reviewed, or confirmed.
    Established(&'static str),
    Rejected(Value),
    /// Rejected, then inferred again on new evidence: needs review again (OBS-39-20).
    Reopened(Value),
}

#[derive(sqlx::FromRow)]
struct ReviewRow {
    review_id: Uuid,
    subject_id: Uuid,
    relation: String,
    object_id: Uuid,
    reviewer: Uuid,
    decision: String,
    reason: String,
    evidence_fingerprint: String,
    decided_at: DateTime<Utc>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(json!({ "error": code }))).into_response()
}

fn store_error(e: sqlx::Error) -> Response {
    tracing::error!(error = %e, "guidance store error");
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal")
}

fn access_error(e: AccessError) -> Response {
    match e {
        AccessError::Unauthorized => error(StatusCode::UNAUTHORIZED, "unauthorized"),
        AccessError::Unavailable => error(StatusCode::SERVICE_UNAVAILABLE, "catalog_unavailable"),
    }
}

fn not_found() -> Response {
    error(StatusCode::NOT_FOUND, "not_found")
}

const EDGE_COLUMNS: &str = "assertion_id, subject_id, relation, object_id, qualifier, source, \
     source_ref, confidence, valid_from, valid_from_basis, valid_to, recorded_at, evidence, \
     evidence_fingerprint";

/// Per-request view: as-of time, authorization cache, entity cache.
struct View<'a> {
    state: &'a AppState,
    access: &'a GuidanceAccess,
    headers: &'a HeaderMap,
    tenant: Uuid,
    at: Option<DateTime<Utc>>,
    visible: HashMap<Uuid, bool>,
    entities: HashMap<Uuid, Entity>,
}

impl<'a> View<'a> {
    fn as_of_sql(&self) -> &'static str {
        if self.at.is_some() {
            " AND valid_from <= $3 AND (valid_to IS NULL OR valid_to > $3) AND recorded_at <= $3"
        } else {
            " AND valid_to IS NULL"
        }
    }

    async fn entity(&mut self, id: Uuid) -> sqlx::Result<Option<Entity>> {
        if let Some(e) = self.entities.get(&id) {
            return Ok(Some(e.clone()));
        }
        let e: Option<Entity> = sqlx::query_as(
            "SELECT entity_id, kind, canonical_key, display_name FROM alerting.guidance_entities \
             WHERE tenant_id = $1 AND entity_id = $2",
        )
        .bind(self.tenant)
        .bind(id)
        .fetch_optional(&self.state.db)
        .await?;
        if let Some(e) = &e {
            self.entities.insert(id, e.clone());
        }
        Ok(e)
    }

    async fn edges(&self, id: Uuid) -> sqlx::Result<Vec<EdgeRow>> {
        let sql = format!(
            "SELECT {EDGE_COLUMNS} FROM alerting.guidance_assertions \
             WHERE tenant_id = $1 AND (subject_id = $2 OR object_id = $2){} \
             ORDER BY relation, valid_from, assertion_id",
            self.as_of_sql()
        );
        let q = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(self.tenant)
            .bind(id);
        match self.at {
            Some(at) => q.bind(at).fetch_all(&self.state.db).await,
            None => q.fetch_all(&self.state.db).await,
        }
    }

    /// Whether the caller may see `e`. Changes and incidents follow the services they touch.
    async fn can_see(&mut self, e: &Entity) -> Result<bool, Response> {
        if let Some(v) = self.visible.get(&e.entity_id) {
            return Ok(*v);
        }
        let v = match e.kind.as_str() {
            "service" | "journey" | "provider" => match e.canonical_key.parse::<Uuid>() {
                Ok(id) => self
                    .access
                    .visible(self.headers, &e.kind, id)
                    .await
                    .map_err(access_error)?,
                // A provider known only by name has no catalog record to authorize against.
                Err(_) => e.kind == "provider",
            },
            "change" | "incident" => {
                let services: Vec<Uuid> = sqlx::query_scalar(
                    "SELECT o.entity_id FROM alerting.guidance_assertions a \
                     JOIN alerting.guidance_entities o ON o.entity_id = \
                       CASE WHEN a.relation = 'changed_by' THEN a.subject_id ELSE a.object_id END \
                     WHERE a.tenant_id = $1 AND o.kind = 'service' \
                       AND ((a.relation = 'changed_by' AND a.object_id = $2) \
                         OR (a.relation = 'affected' AND a.subject_id = $2))",
                )
                .bind(self.tenant)
                .bind(e.entity_id)
                .fetch_all(&self.state.db)
                .await
                .map_err(store_error)?;
                let mut any = services.is_empty();
                for s in services {
                    if let Some(svc) = self.entity(s).await.map_err(store_error)?
                        && Box::pin(self.can_see(&svc)).await?
                    {
                        any = true;
                        break;
                    }
                }
                any
            }
            _ => true,
        };
        self.visible.insert(e.entity_id, v);
        Ok(v)
    }

    /// The entity, or the same 404 for missing and hidden.
    async fn readable(&mut self, id: Uuid) -> Result<Entity, Response> {
        let e = self
            .entity(id)
            .await
            .map_err(store_error)?
            .ok_or_else(not_found)?;
        if !self.can_see(&e).await? {
            return Err(not_found());
        }
        Ok(e)
    }

    /// Visible edges of `id`, with the entity on the other end.
    async fn visible_edges(&mut self, id: Uuid) -> Result<Vec<(EdgeRow, Entity)>, Response> {
        let rows = self.edges(id).await.map_err(store_error)?;
        let mut out = Vec::new();
        for r in rows {
            let other = if r.subject_id == id {
                r.object_id
            } else {
                r.subject_id
            };
            let Some(o) = self.entity(other).await.map_err(store_error)? else {
                continue;
            };
            if self.can_see(&o).await? {
                out.push((r, o));
            }
        }
        Ok(out)
    }

    /// Review state per assertion of `rows`. Decisions apply to the relationship (subject,
    /// relation, object), so a later interval of a rejected inference stays rejected.
    async fn reviews(&self, rows: &[&EdgeRow]) -> sqlx::Result<HashMap<Uuid, Review>> {
        let inferred: Vec<&EdgeRow> = rows
            .iter()
            .copied()
            .filter(|r| r.confidence == "inferred")
            .collect();
        let mut out: HashMap<Uuid, Review> = rows
            .iter()
            .map(|r| {
                let s = if r.confidence == "inferred" {
                    "unreviewed"
                } else {
                    "not_reviewable"
                };
                (r.assertion_id, Review::Established(s))
            })
            .collect();
        if inferred.is_empty() {
            return Ok(out);
        }
        let subjects: Vec<Uuid> = inferred.iter().map(|r| r.subject_id).collect();
        let at = self.at.unwrap_or_else(Utc::now);
        let decisions: Vec<ReviewRow> = sqlx::query_as(
            "SELECT DISTINCT ON (subject_id, relation, object_id) review_id, subject_id, relation, \
                    object_id, reviewer, decision, reason, evidence_fingerprint, decided_at \
             FROM alerting.guidance_reviews \
             WHERE tenant_id = $1 AND subject_id = ANY($2) AND decided_at <= $3 \
             ORDER BY subject_id, relation, object_id, decided_at DESC, review_id DESC",
        )
        .bind(self.tenant)
        .bind(&subjects)
        .bind(at)
        .fetch_all(&self.state.db)
        .await?;
        let ids: Vec<Uuid> = inferred.iter().map(|r| r.assertion_id).collect();
        let latest: HashMap<Uuid, (String, DateTime<Utc>)> =
            sqlx::query_as::<_, (Uuid, String, DateTime<Utc>)>(
                "SELECT assertion_id, evidence_fingerprint, last_seen \
                 FROM alerting.guidance_inference_state \
                 WHERE tenant_id = $1 AND assertion_id = ANY($2)",
            )
            .bind(self.tenant)
            .bind(&ids)
            .fetch_all(&self.state.db)
            .await?
            .into_iter()
            .map(|(id, fp, seen)| (id, (fp, seen)))
            .collect();
        for r in inferred {
            let Some(d) = decisions.iter().find(|d| {
                d.subject_id == r.subject_id
                    && d.relation == r.relation
                    && d.object_id == r.object_id
            }) else {
                continue;
            };
            let record = json!({
                "review_id": d.review_id,
                "decision": d.decision,
                "reviewer": d.reviewer,
                "reason": d.reason,
                "decided_at": d.decided_at,
                "evidence_fingerprint": d.evidence_fingerprint,
            });
            let state = if d.decision == "confirm" {
                Review::Established("confirmed")
            } else {
                let (fp, seen) = latest
                    .get(&r.assertion_id)
                    .cloned()
                    .unwrap_or((r.evidence_fingerprint.clone(), r.valid_from));
                if fp != d.evidence_fingerprint && seen > d.decided_at && seen <= at {
                    Review::Reopened(record)
                } else {
                    Review::Rejected(record)
                }
            };
            out.insert(r.assertion_id, state);
        }
        Ok(out)
    }

    /// Sources with no complete run started at or before the as-of time.
    async fn unrecorded_sources(&self) -> sqlx::Result<Vec<String>> {
        let at = self.at.unwrap_or_else(Utc::now);
        let done: Vec<String> = sqlx::query_scalar(
            "SELECT DISTINCT source FROM alerting.guidance_source_runs \
             WHERE tenant_id = $1 AND complete AND started_at <= $2",
        )
        .bind(self.tenant)
        .bind(at)
        .fetch_all(&self.state.db)
        .await?;
        Ok(["catalog", "control", "alerting", "telemetry"]
            .iter()
            .filter(|s| !done.iter().any(|d| d == *s))
            .map(|s| (*s).to_owned())
            .collect())
    }
}

/// An edge as the read reports it. In an as-of read the edge is rendered as it was recorded
/// at that time, so a later closure cannot change it; closures since then are reported
/// separately under `since_then` (OBS-39-13).
fn edge_json(
    r: &EdgeRow,
    me: Uuid,
    other: &Entity,
    conflict: bool,
    as_of: bool,
    review: &Review,
) -> Value {
    let mut v = json!({
        "assertion_id": r.assertion_id,
        "relation": r.relation,
        "direction": if r.subject_id == me { "outgoing" } else { "incoming" },
        "qualifier": r.qualifier,
        "entity": other,
        "source": r.source,
        "source_ref": r.source_ref,
        "confidence": r.confidence,
        "valid_from": r.valid_from,
        "valid_from_basis": r.valid_from_basis,
        "valid_to": r.valid_to,
        "current": r.valid_to.is_none(),
        "recorded_at": r.recorded_at,
        "conflict": conflict,
        "evidence": r.evidence,
    });
    match review {
        Review::Established(s) => v["review_state"] = json!(s),
        Review::Rejected(d) => {
            v["review_state"] = json!("rejected");
            v["review"] = d.clone();
        }
        Review::Reopened(d) => {
            v["review_state"] = json!("reopened");
            v["review"] = d.clone();
        }
    }
    if as_of {
        strip_live_fields(&mut v);
    }
    v
}

fn strip_live_fields(v: &mut Value) {
    if let Some(o) = v.as_object_mut() {
        o.remove("valid_to");
        o.remove("current");
    }
}

/// Edges of an as-of read that have been closed since: `{assertion_id, valid_to}`.
fn since_then<'r>(rows: impl Iterator<Item = &'r EdgeRow>) -> Vec<Value> {
    let mut out: Vec<Value> = rows
        .filter_map(|r| {
            r.valid_to
                .map(|t| json!({"assertion_id": r.assertion_id, "valid_to": t, "current": false}))
        })
        .collect();
    out.sort_by_key(|v| v["assertion_id"].as_str().map(str::to_owned));
    out
}

/// Owner claims for the same subject and role from different principals conflict (OBS-39-11).
fn conflicting(edges: &[(EdgeRow, Entity)], me: Uuid) -> HashSet<Uuid> {
    let mut by_role: BTreeMap<&str, BTreeSet<Uuid>> = BTreeMap::new();
    for (r, _) in edges
        .iter()
        .filter(|(r, _)| r.relation == "owned_by" && r.subject_id == me)
    {
        by_role.entry(&r.qualifier).or_default().insert(r.object_id);
    }
    edges
        .iter()
        .filter(|(r, _)| {
            r.relation == "owned_by"
                && r.subject_id == me
                && by_role
                    .get(r.qualifier.as_str())
                    .is_some_and(|s| s.len() > 1)
        })
        .map(|(r, _)| r.assertion_id)
        .collect()
}

#[derive(Debug, Deserialize)]
pub struct AtQuery {
    pub at: Option<DateTime<Utc>>,
}

/// `GET /v1/guidance/entities/{entity_id}?at=`: the entity and its relationships.
pub async fn handle_entity(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(access): Extension<GuidanceAccess>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
    Query(q): Query<AtQuery>,
) -> Response {
    let mut v = View {
        state: &state,
        access: &access,
        headers: &headers,
        tenant: ctx.tenant_id,
        at: q.at,
        visible: HashMap::new(),
        entities: HashMap::new(),
    };
    let e = match v.readable(id).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let unrecorded = match v.unrecorded_sources().await {
        Ok(u) => u,
        Err(e) => return store_error(e),
    };
    if unrecorded.len() == 4 {
        return Json(json!({
            "entity": e,
            "at": q.at,
            "context": "no_recorded_context",
            "relationships": [],
            "relations": {},
        }))
        .into_response();
    }
    let edges = match v.visible_edges(id).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let reviews = match v
        .reviews(&edges.iter().map(|(r, _)| r).collect::<Vec<_>>())
        .await
    {
        Ok(x) => x,
        Err(e) => return store_error(e),
    };
    let review_of = |r: &EdgeRow| {
        reviews
            .get(&r.assertion_id)
            .cloned()
            .unwrap_or(Review::Established("not_reviewable"))
    };
    // A rejected or reopened inference is never returned as established (OBS-39-18, 20).
    let (edges, held): (Vec<_>, Vec<_>) = edges
        .into_iter()
        .partition(|(r, _)| matches!(review_of(r), Review::Established(_)));
    let conflicts = conflicting(&edges, id);
    let relationships: Vec<Value> = edges
        .iter()
        .map(|(r, o)| {
            let c = conflicts.contains(&r.assertion_id);
            edge_json(r, id, o, c, q.at.is_some(), &review_of(r))
        })
        .collect();
    let held_json = |want: &str| -> Vec<Value> {
        held.iter()
            .map(|(r, o)| edge_json(r, id, o, false, q.at.is_some(), &review_of(r)))
            .filter(|e| e["review_state"] == want)
            .collect()
    };
    let rejected = held_json("rejected");
    let reopened = held_json("reopened");
    // A relation without a visible edge is unknown, never "none" (OBS-39-08).
    let relations: BTreeMap<&str, &str> = if e.kind == "service" || e.kind == "unregistered_service"
    {
        SERVICE_RELATIONS
            .iter()
            .map(|rel| {
                let known = edges.iter().any(|(r, _)| r.relation == *rel);
                (*rel, if known { "known" } else { "unknown" })
            })
            .collect()
    } else {
        BTreeMap::new()
    };
    let mut body = json!({
        "entity": e,
        "at": q.at,
        "context": "recorded",
        "unrecorded_sources": unrecorded,
        "relationships": relationships,
        "relations": relations,
        "rejected": rejected,
        "reopened": reopened,
    });
    if q.at.is_some() {
        body["since_then"] = Value::from(since_then(edges.iter().map(|(r, _)| r)));
    }
    Json(body).into_response()
}

#[derive(Debug, Deserialize)]
pub struct GraphQuery {
    pub from: Uuid,
    pub depth: Option<u8>,
    pub relations: Option<String>,
    pub at: Option<DateTime<Utc>>,
}

/// `GET /v1/guidance/graph?from=&depth=1..3&relations=&at=`: reachable visible nodes and edges.
pub async fn handle_graph(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(access): Extension<GuidanceAccess>,
    headers: HeaderMap,
    Query(q): Query<GraphQuery>,
) -> Response {
    let depth = q.depth.unwrap_or(2);
    if !(1..=3).contains(&depth) {
        return error(StatusCode::BAD_REQUEST, "invalid_depth");
    }
    let filter: Option<HashSet<String>> = q.relations.as_deref().map(|r| {
        r.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect()
    });
    if filter
        .as_ref()
        .is_some_and(|f| f.iter().any(|r| !RELATIONS.contains(&r.as_str())))
    {
        return error(StatusCode::BAD_REQUEST, "unknown_relation");
    }
    let mut v = View {
        state: &state,
        access: &access,
        headers: &headers,
        tenant: ctx.tenant_id,
        at: q.at,
        visible: HashMap::new(),
        entities: HashMap::new(),
    };
    let start = match v.readable(q.from).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    match v.unrecorded_sources().await {
        Ok(u) if u.len() == 4 => {
            return Json(
                json!({"from": start, "at": q.at, "context": "no_recorded_context",
                               "nodes": [start], "edges": []}),
            )
            .into_response();
        }
        Ok(_) => {}
        Err(e) => return store_error(e),
    }
    let mut nodes: BTreeMap<Uuid, Entity> = BTreeMap::from([(start.entity_id, start.clone())]);
    let mut edges: BTreeMap<Uuid, Value> = BTreeMap::new();
    let mut queue = VecDeque::from([(start.entity_id, 0u8)]);
    let mut truncated = false;
    let mut closed: BTreeMap<Uuid, EdgeRow> = BTreeMap::new();
    while let Some((id, d)) = queue.pop_front() {
        if d >= depth {
            continue;
        }
        let found = match v.visible_edges(id).await {
            Ok(x) => x,
            Err(r) => return r,
        };
        let reviews = match v
            .reviews(&found.iter().map(|(r, _)| r).collect::<Vec<_>>())
            .await
        {
            Ok(x) => x,
            Err(e) => return store_error(e),
        };
        let found: Vec<(EdgeRow, Entity)> = found
            .into_iter()
            .filter(|(r, _)| {
                matches!(
                    reviews.get(&r.assertion_id),
                    Some(Review::Established(_)) | None
                )
            })
            .collect();
        let conflicts = conflicting(&found, id);
        for (r, o) in &found {
            if filter.as_ref().is_some_and(|f| !f.contains(&r.relation)) {
                continue;
            }
            if !nodes.contains_key(&o.entity_id) {
                if nodes.len() >= MAX_GRAPH_NODES {
                    truncated = true;
                    continue;
                }
                nodes.insert(o.entity_id, o.clone());
                queue.push_back((o.entity_id, d + 1));
            }
            if r.valid_to.is_some() {
                closed.entry(r.assertion_id).or_insert_with(|| r.clone());
            }
            edges.entry(r.assertion_id).or_insert_with(|| {
                let mut e = json!({
                    "assertion_id": r.assertion_id,
                    "subject_id": r.subject_id,
                    "relation": r.relation,
                    "object_id": r.object_id,
                    "qualifier": r.qualifier,
                    "source": r.source,
                    "source_ref": r.source_ref,
                    "confidence": r.confidence,
                    "valid_from": r.valid_from,
                    "valid_to": r.valid_to,
                    "current": r.valid_to.is_none(),
                    "conflict": conflicts.contains(&r.assertion_id),
                });
                if q.at.is_some() {
                    strip_live_fields(&mut e);
                }
                e
            });
        }
    }
    let mut body = json!({
        "from": start,
        "at": q.at,
        "context": "recorded",
        "depth": depth,
        "nodes": nodes.into_values().collect::<Vec<_>>(),
        "edges": edges.into_values().collect::<Vec<_>>(),
        "truncated": truncated,
    });
    if q.at.is_some() {
        body["since_then"] = Value::from(since_then(closed.values()));
    }
    Json(body).into_response()
}

#[derive(Debug, Deserialize)]
pub struct LookupQuery {
    pub kind: String,
    pub key: String,
}

/// `GET /v1/guidance/entities?kind=&key=`: lookup by canonical source key.
pub async fn handle_lookup(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(access): Extension<GuidanceAccess>,
    headers: HeaderMap,
    Query(q): Query<LookupQuery>,
) -> Response {
    if !KINDS.contains(&q.kind.as_str()) {
        return error(StatusCode::BAD_REQUEST, "unknown_kind");
    }
    let id: Option<Uuid> = match sqlx::query_scalar(
        "SELECT entity_id FROM alerting.guidance_entities \
         WHERE tenant_id = $1 AND kind = $2 AND canonical_key = $3",
    )
    .bind(ctx.tenant_id)
    .bind(&q.kind)
    .bind(&q.key)
    .fetch_optional(&state.db)
    .await
    {
        Ok(i) => i,
        Err(e) => return store_error(e),
    };
    let Some(id) = id else {
        return not_found();
    };
    let mut v = View {
        state: &state,
        access: &access,
        headers: &headers,
        tenant: ctx.tenant_id,
        at: None,
        visible: HashMap::new(),
        entities: HashMap::new(),
    };
    match v.readable(id).await {
        Ok(e) => Json(e).into_response(),
        Err(r) => r,
    }
}

// --- Approved agents (W4b, OBS-39-04, 15, 16, 17) ---------------------------------------------

/// Purposes an agent can be approved for, and the relations each one releases.
pub const PURPOSES: &[(&str, &[&str])] = &[
    (
        "incident_response",
        &["owned_by", "changed_by", "affected", "part_of_journey"],
    ),
    (
        "dependency_analysis",
        &["calls", "provided_by", "part_of_journey"],
    ),
];

fn released(purpose: &str) -> Option<&'static [&'static str]> {
    PURPOSES
        .iter()
        .find(|(p, _)| *p == purpose)
        .map(|(_, r)| *r)
}

#[derive(Debug, Deserialize)]
pub struct ConsumerBody {
    pub approved_purposes: Vec<String>,
}

/// `PUT /v1/guidance/consumers/{api_key_id}`: a signed-in tenant admin approves an agent key's
/// purposes. An empty list withdraws every approval.
pub async fn handle_put_consumer(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path(api_key_id): Path<Uuid>,
    Json(body): Json<ConsumerBody>,
) -> Response {
    let Some(admin) = ctx.user_id.filter(|_| ctx.role == "tenant_admin") else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    let mut purposes: Vec<String> = body.approved_purposes;
    purposes.sort();
    purposes.dedup();
    if purposes.iter().any(|p| released(p).is_none()) {
        return error(StatusCode::BAD_REQUEST, "unknown_purpose");
    }
    let res = sqlx::query(
        "INSERT INTO alerting.guidance_consumers \
           (tenant_id, api_key_id, approved_purposes, approved_by, updated_at) \
         VALUES ($1, $2, $3, $4, now()) \
         ON CONFLICT (tenant_id, api_key_id) DO UPDATE SET \
           approved_purposes = EXCLUDED.approved_purposes, approved_by = EXCLUDED.approved_by, \
           updated_at = now()",
    )
    .bind(ctx.tenant_id)
    .bind(api_key_id)
    .bind(&purposes)
    .bind(admin)
    .execute(&state.db)
    .await;
    match res {
        Ok(_) => Json(json!({
            "api_key_id": api_key_id,
            "approved_purposes": purposes,
            "approved_by": admin,
        }))
        .into_response(),
        Err(e) => store_error(e),
    }
}

#[derive(Debug, Deserialize)]
pub struct ContextQuery {
    pub service_id: Option<String>,
    pub purpose: Option<String>,
    pub at: Option<DateTime<Utc>>,
}

/// Provenance of one context element, or none when any part is missing.
fn provenance(r: &EdgeRow) -> Option<Value> {
    if r.source.is_empty() || r.source_ref.is_empty() || r.confidence.is_empty() {
        return None;
    }
    Some(json!({
        "source": r.source,
        "source_ref": r.source_ref,
        "confidence": r.confidence,
        "valid_from": r.valid_from,
        "recorded_at": r.recorded_at,
    }))
}

/// What an agent can follow back to the source record.
fn evidence_refs(r: &EdgeRow, other: &Entity) -> Value {
    let mut refs = vec![json!({"kind": "record", "ref": r.source_ref})];
    match other.kind.as_str() {
        "change" => refs.push(json!({"kind": "deployment", "id": other.canonical_key})),
        "incident" => refs.push(json!({"kind": "incident", "id": other.canonical_key})),
        _ => {}
    }
    if let Some(traces) = r.evidence["trace_ids"].as_array() {
        for t in traces {
            refs.push(json!({"kind": "trace", "id": t}));
        }
    }
    Value::from(refs)
}

/// `GET /v1/guidance/context?service_id=&purpose=&at=`: purpose-scoped context for an approved
/// agent key. The purpose is checked before any lookup, so a denial says nothing about the
/// service; a hidden and a missing service answer the same 404 (OBS-39-16, 17).
pub async fn handle_context(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    key: Option<Extension<crate::middleware::auth::ApiKeyIdentity>>,
    Extension(access): Extension<GuidanceAccess>,
    headers: HeaderMap,
    Query(q): Query<ContextQuery>,
) -> Response {
    let Some(Extension(crate::middleware::auth::ApiKeyIdentity(key_id))) = key else {
        return error(StatusCode::FORBIDDEN, "api_key_required");
    };
    let purpose = q.purpose.unwrap_or_default();
    let approved: Option<Vec<String>> = match sqlx::query_scalar(
        "SELECT approved_purposes FROM alerting.guidance_consumers \
         WHERE tenant_id = $1 AND api_key_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(key_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(a) => a,
        Err(e) => return store_error(e),
    };
    let Some(relations) =
        released(&purpose).filter(|_| approved.is_some_and(|a| a.contains(&purpose)))
    else {
        return error(StatusCode::FORBIDDEN, "purpose_not_approved");
    };
    let Some(service_key) = q.service_id.filter(|s| s.parse::<Uuid>().is_ok()) else {
        return not_found();
    };
    let id: Option<Uuid> = match sqlx::query_scalar(
        "SELECT entity_id FROM alerting.guidance_entities \
         WHERE tenant_id = $1 AND kind = 'service' AND canonical_key = $2",
    )
    .bind(ctx.tenant_id)
    .bind(&service_key)
    .fetch_optional(&state.db)
    .await
    {
        Ok(i) => i,
        Err(e) => return store_error(e),
    };
    let Some(id) = id else {
        return not_found();
    };
    let mut v = View {
        state: &state,
        access: &access,
        headers: &headers,
        tenant: ctx.tenant_id,
        at: q.at,
        visible: HashMap::new(),
        entities: HashMap::new(),
    };
    let service = match v.readable(id).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let edges = match v.visible_edges(id).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let reviews = match v
        .reviews(&edges.iter().map(|(r, _)| r).collect::<Vec<_>>())
        .await
    {
        Ok(x) => x,
        Err(e) => return store_error(e),
    };
    let mut complete = true;
    let mut elements = Vec::new();
    for (r, other) in &edges {
        if !relations.contains(&r.relation.as_str())
            || !matches!(
                reviews.get(&r.assertion_id),
                Some(Review::Established(_)) | None
            )
        {
            continue;
        }
        // Never emit an element without provenance (OBS-39-15).
        let Some(prov) = provenance(r) else {
            complete = false;
            continue;
        };
        elements.push(json!({
            "relation": r.relation,
            "direction": if r.subject_id == id { "outgoing" } else { "incoming" },
            "qualifier": r.qualifier,
            "entity": other,
            "current": r.valid_to.is_none(),
            "provenance": prov,
            "evidence_refs": evidence_refs(r, other),
        }));
    }
    Json(json!({
        "service": service,
        "purpose": purpose,
        "at": q.at,
        "complete": complete,
        "elements": elements,
    }))
    .into_response()
}

// --- Review of inferred relationships (W4b, OBS-39-05, 18, 19, 20) ----------------------------

#[derive(Debug, Deserialize)]
pub struct ReviewBody {
    pub decision: String,
    pub reason: Option<String>,
}

/// `POST /v1/guidance/relationships/{assertion_id}/review`: a signed-in tenant admin or an
/// operational owner of the subject service rejects or confirms an inferred relationship,
/// with a reason.
pub async fn handle_review(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(access): Extension<GuidanceAccess>,
    headers: HeaderMap,
    Path(assertion_id): Path<Uuid>,
    Json(body): Json<ReviewBody>,
) -> Response {
    let row: Option<EdgeRow> = match sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {EDGE_COLUMNS} FROM alerting.guidance_assertions \
         WHERE tenant_id = $1 AND assertion_id = $2"
    )))
    .bind(ctx.tenant_id)
    .bind(assertion_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(r) => r,
        Err(e) => return store_error(e),
    };
    let Some(row) = row else {
        return not_found();
    };
    let mut v = View {
        state: &state,
        access: &access,
        headers: &headers,
        tenant: ctx.tenant_id,
        at: None,
        visible: HashMap::new(),
        entities: HashMap::new(),
    };
    // Both ends must be visible to the reviewer, or the relationship does not exist for them.
    if let Err(r) = v.readable(row.subject_id).await {
        return r;
    }
    if let Err(r) = v.readable(row.object_id).await {
        return r;
    }
    let Some(user) = ctx.user_id else {
        return error(StatusCode::FORBIDDEN, "forbidden");
    };
    if ctx.role != "tenant_admin" {
        let owner: bool = match sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM alerting.guidance_assertions a \
               JOIN alerting.guidance_entities o ON o.entity_id = a.object_id \
               WHERE a.tenant_id = $1 AND a.subject_id = $2 AND a.relation = 'owned_by' \
                 AND a.source = 'catalog' AND a.qualifier = 'operational_owner' \
                 AND a.valid_to IS NULL AND o.canonical_key = $3)",
        )
        .bind(ctx.tenant_id)
        .bind(row.subject_id)
        .bind(format!("user:{user}"))
        .fetch_one(&state.db)
        .await
        {
            Ok(b) => b,
            Err(e) => return store_error(e),
        };
        if !owner {
            return error(StatusCode::FORBIDDEN, "forbidden");
        }
    }
    if !matches!(body.decision.as_str(), "reject" | "confirm") {
        return error(StatusCode::BAD_REQUEST, "invalid_decision");
    }
    let reason = body.reason.unwrap_or_default();
    let reason = reason.trim();
    if reason.is_empty() || reason.chars().count() > 1000 {
        return error(StatusCode::BAD_REQUEST, "reason_required");
    }
    if row.confidence != "inferred" {
        return error(StatusCode::CONFLICT, "not_inferred");
    }
    // The decision is bound to the evidence the reviewer saw.
    let fingerprint: String = match sqlx::query_scalar::<_, String>(
        "SELECT evidence_fingerprint FROM alerting.guidance_inference_state \
         WHERE tenant_id = $1 AND assertion_id = $2",
    )
    .bind(ctx.tenant_id)
    .bind(assertion_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(f) => f.unwrap_or_else(|| row.evidence_fingerprint.clone()),
        Err(e) => return store_error(e),
    };
    let saved: Result<(Uuid, DateTime<Utc>), _> = sqlx::query_as(
        "INSERT INTO alerting.guidance_reviews \
           (tenant_id, assertion_id, subject_id, relation, object_id, reviewer, decision, reason, \
            evidence_fingerprint) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING review_id, decided_at",
    )
    .bind(ctx.tenant_id)
    .bind(assertion_id)
    .bind(row.subject_id)
    .bind(&row.relation)
    .bind(row.object_id)
    .bind(user)
    .bind(&body.decision)
    .bind(reason)
    .bind(&fingerprint)
    .fetch_one(&state.db)
    .await;
    match saved {
        Ok((review_id, decided_at)) => (
            StatusCode::CREATED,
            Json(json!({
                "review_id": review_id,
                "assertion_id": assertion_id,
                "decision": body.decision,
                "reviewer": user,
                "reason": reason,
                "decided_at": decided_at,
                "evidence_fingerprint": fingerprint,
            })),
        )
            .into_response(),
        Err(e) => store_error(e),
    }
}
