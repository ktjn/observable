//! Registers alerting's rules and objectives in the catalog asset registry (phase 1 S6,
//! OBS-34-01). A periodic full sync per tenant and kind is the only path: the catalog retires
//! what a sync no longer lists, so deleted rules and rules whose service lost its binding stop
//! counting as coverage without a per-write call that could be lost.

use std::collections::BTreeMap;
use std::time::Duration;

use serde::Serialize;
use uuid::Uuid;

use crate::alerts::OwnerRef;

pub const OWNING_COMPONENT: &str = "observable-alerting";

#[derive(Clone)]
pub struct CatalogClient {
    url: String,
    token: String,
    http: reqwest::Client,
}

impl CatalogClient {
    pub fn new(url: impl Into<String>, token: impl Into<String>, http: reqwest::Client) -> Self {
        Self {
            url: url.into().trim_end_matches('/').to_owned(),
            token: token.into(),
            http,
        }
    }

    async fn sync(&self, tenant_id: Uuid, kind: &str, assets: &[SyncAsset]) -> anyhow::Result<()> {
        let resp = self
            .http
            .post(format!("{}/internal/catalog/assets/sync", self.url))
            .header("X-Internal-Token", &self.token)
            .timeout(Duration::from_secs(10))
            .json(&serde_json::json!({
                "tenant_id": tenant_id,
                "owning_component": OWNING_COMPONENT,
                "kind": kind,
                "assets": assets,
            }))
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("catalog sync returned {}", resp.status());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SyncAsset {
    pub external_id: String,
    /// Catalog service id, for assets already bound to the catalog (approved objectives).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_id: Option<Uuid>,
    /// Telemetry identity, for assets that name a service (rules, legacy SLOs).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_name: Option<String>,
    pub environment: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<OwnerRef>,
}

type Owner = (Option<String>, Option<Uuid>);

fn owner((kind, id): Owner) -> Option<OwnerRef> {
    Some(OwnerRef {
        principal_kind: kind?,
        principal_id: id?,
    })
}

/// Alert rules and objectives per tenant, keyed by asset kind. Every tenant with any rule or
/// objective gets both kinds, so a kind that became empty still retires its stale assets.
pub async fn collect(
    db: &sqlx::PgPool,
) -> Result<BTreeMap<Uuid, BTreeMap<&'static str, Vec<SyncAsset>>>, sqlx::Error> {
    let tenants: Vec<Uuid> = sqlx::query_scalar(
        "SELECT tenant_id FROM alert_rules UNION SELECT tenant_id FROM slo_definitions \
         UNION SELECT tenant_id FROM service_objectives",
    )
    .fetch_all(db)
    .await?;
    let mut out: BTreeMap<Uuid, BTreeMap<&'static str, Vec<SyncAsset>>> = tenants
        .into_iter()
        .map(|t| {
            (
                t,
                BTreeMap::from([("alert_rule", vec![]), ("objective", vec![])]),
            )
        })
        .collect();

    // A rule without a service cannot be attributed and is left out (retired if it was listed).
    type RuleRow = (Uuid, Uuid, String, String, Option<String>, Option<Uuid>);
    let rules: Vec<RuleRow> = sqlx::query_as(
        "SELECT tenant_id, rule_id, name, service_name, owner_kind, owner_id FROM alert_rules \
         WHERE service_name IS NOT NULL AND btrim(service_name) <> '' ORDER BY rule_id",
    )
    .fetch_all(db)
    .await?;
    for (tenant, id, name, service, kind, owner_id) in rules {
        let asset = asset(id, service, String::new(), name, owner((kind, owner_id)));
        push(&mut out, tenant, "alert_rule", asset);
    }

    type SloRow = (
        Uuid,
        Uuid,
        String,
        String,
        String,
        f64,
        String,
        Option<String>,
        Option<Uuid>,
    );
    let slos: Vec<SloRow> = sqlx::query_as(
        "SELECT tenant_id, slo_id, service_name, environment, sli_type, target, description, \
         owner_kind, owner_id FROM slo_definitions ORDER BY slo_id",
    )
    .fetch_all(db)
    .await?;
    for (tenant, id, service, env, sli, target, description, kind, owner_id) in slos {
        let name = if description.trim().is_empty() {
            format!("{service} {sli} {target}")
        } else {
            description
        };
        let asset = asset(id, service, env, name, owner((kind, owner_id)));
        push(&mut out, tenant, "objective", asset);
    }

    // Catalog-bound objectives (phase 1 C3) count once approved; a draft or an objective whose
    // approval was cleared by an edit is left out, so its asset is retired. The creator owns it.
    type ObjectiveRow = (Uuid, Uuid, Uuid, String, String, f64, String, Option<Uuid>);
    let objectives: Vec<ObjectiveRow> = sqlx::query_as(
        "SELECT tenant_id, objective_id, service_id, environment_class, sli_type, target, \
         description, created_by FROM service_objectives WHERE approved_at IS NOT NULL \
         ORDER BY objective_id",
    )
    .fetch_all(db)
    .await?;
    for (tenant, id, service_id, class, sli, target, description, created_by) in objectives {
        let name = if description.trim().is_empty() {
            format!("{sli} {target} {class}")
        } else {
            description
        };
        let owner = created_by.map(|principal_id| OwnerRef {
            principal_kind: "user".into(),
            principal_id,
        });
        push(
            &mut out,
            tenant,
            "objective",
            SyncAsset {
                external_id: id.to_string(),
                service_id: Some(service_id),
                service_name: None,
                environment: String::new(),
                name: name.chars().take(200).collect(),
                owner,
            },
        );
    }
    Ok(out)
}

fn asset(
    id: Uuid,
    service_name: String,
    environment: String,
    name: String,
    owner: Option<OwnerRef>,
) -> SyncAsset {
    SyncAsset {
        external_id: id.to_string(),
        service_id: None,
        service_name: Some(service_name.trim().to_owned()),
        environment: environment.trim().to_owned(),
        name: name.chars().take(200).collect(),
        owner,
    }
}

fn push(
    out: &mut BTreeMap<Uuid, BTreeMap<&'static str, Vec<SyncAsset>>>,
    tenant: Uuid,
    kind: &'static str,
    asset: SyncAsset,
) {
    out.entry(tenant)
        .or_default()
        .entry(kind)
        .or_default()
        .push(asset);
}

/// One full sync. A failing tenant does not stop the others; the count of failures is returned.
pub async fn sync_once(db: &sqlx::PgPool, catalog: &CatalogClient) -> anyhow::Result<usize> {
    let mut failures = 0;
    for (tenant, kinds) in collect(db).await? {
        for (kind, assets) in kinds {
            if let Err(e) = catalog.sync(tenant, kind, &assets).await {
                failures += 1;
                tracing::warn!(%tenant, kind, error = %e, "catalog asset sync failed");
            }
        }
    }
    Ok(failures)
}

pub async fn start_sync_worker(db: sqlx::PgPool, catalog: CatalogClient, interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticker.tick().await;
        if let Err(e) = sync_once(&db, &catalog).await {
            tracing::warn!(error = %e, "catalog asset sync skipped");
        }
    }
}
