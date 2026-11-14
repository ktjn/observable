//! Dependency exposure and business impact assessment (`map-dependencies-and-impact`,
//! Phase 7 S2: OBS-10-01, OBS-10-02, OBS-10-07).
//!
//! `GET /v1/impact/dependencies?dependency=` returns all business services depending on
//! a service or external provider, preserving unconfirmed mappings with an explicit
//! `unconfirmed` status rather than omitting them.
//! `POST /v1/impact/assess` computes impact priority from both technical symptoms and
//! business exposure (critical processes/journeys), and separates evidence-confirmed
//! transaction counts from labeled estimates and explicit `unknown` impact (never `0`).

use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Extension, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependentServiceEdge {
    pub business_service_name: String,
    pub criticality: String,
    pub journeys: Vec<String>,
    pub mapping_confirmed: bool,
    pub mapping_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceBusinessExposure {
    pub service_name: String,
    pub criticality: String,
    pub critical_journeys: Vec<String>,
    pub critical_processes: Vec<String>,
}

#[async_trait::async_trait]
pub trait DependencyImpactSource: Send + Sync {
    async fn dependents_of(
        &self,
        tenant_id: Uuid,
        dependency: &str,
    ) -> Result<Vec<DependentServiceEdge>, ()>;

    async fn service_exposure(
        &self,
        tenant_id: Uuid,
        service_name: &str,
    ) -> Result<Option<ServiceBusinessExposure>, ()>;
}

#[derive(Default)]
pub struct InMemoryDependencyImpactSource {
    edges: Mutex<Vec<(Uuid, String, DependentServiceEdge)>>,
    exposures: Mutex<Vec<(Uuid, ServiceBusinessExposure)>>,
}

impl InMemoryDependencyImpactSource {
    #[allow(dead_code)]
    pub fn new(
        edges: Vec<(Uuid, String, DependentServiceEdge)>,
        exposures: Vec<(Uuid, ServiceBusinessExposure)>,
    ) -> Self {
        Self {
            edges: Mutex::new(edges),
            exposures: Mutex::new(exposures),
        }
    }
}

#[async_trait::async_trait]
impl DependencyImpactSource for InMemoryDependencyImpactSource {
    async fn dependents_of(
        &self,
        tenant_id: Uuid,
        dependency: &str,
    ) -> Result<Vec<DependentServiceEdge>, ()> {
        let guard = self.edges.lock().unwrap();
        Ok(guard
            .iter()
            .filter(|(t, dep, _)| *t == tenant_id && dep.eq_ignore_ascii_case(dependency))
            .map(|(_, _, e)| e.clone())
            .collect())
    }

    async fn service_exposure(
        &self,
        tenant_id: Uuid,
        service_name: &str,
    ) -> Result<Option<ServiceBusinessExposure>, ()> {
        let guard = self.exposures.lock().unwrap();
        Ok(guard
            .iter()
            .find(|(t, exp)| *t == tenant_id && exp.service_name.eq_ignore_ascii_case(service_name))
            .map(|(_, exp)| exp.clone()))
    }
}

#[derive(Debug, Deserialize)]
pub struct DependencyExposureQuery {
    pub dependency: String,
}

#[derive(Debug, Serialize)]
pub struct DependencyExposureItem {
    pub business_service_name: String,
    pub criticality: String,
    pub journeys: Vec<String>,
    pub mapping_status: String,
    pub mapping_source: String,
}

#[derive(Debug, Serialize)]
pub struct DependencyExposureResponse {
    pub dependency: String,
    pub confirmed_count: usize,
    pub unconfirmed_count: usize,
    pub immediate_exposure: Vec<DependencyExposureItem>,
}

pub async fn get_dependency_exposure(
    source: Extension<Arc<dyn DependencyImpactSource>>,
    tenant: Extension<TenantContext>,
    Query(q): Query<DependencyExposureQuery>,
) -> Response {
    let dep = q.dependency.trim().to_string();
    if dep.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "dependency_required" })),
        )
            .into_response();
    }
    let Ok(edges) = source.dependents_of(tenant.tenant_id, &dep).await else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": "dependency_source_unavailable" })),
        )
            .into_response();
    };

    let mut confirmed_count = 0;
    let mut unconfirmed_count = 0;
    let immediate_exposure = edges
        .into_iter()
        .map(|e| {
            let mapping_status = if e.mapping_confirmed {
                confirmed_count += 1;
                "confirmed".to_string()
            } else {
                unconfirmed_count += 1;
                "unconfirmed".to_string()
            };
            DependencyExposureItem {
                business_service_name: e.business_service_name,
                criticality: e.criticality,
                journeys: e.journeys,
                mapping_status,
                mapping_source: e.mapping_source,
            }
        })
        .collect();

    (
        StatusCode::OK,
        Json(DependencyExposureResponse {
            dependency: dep,
            confirmed_count,
            unconfirmed_count,
            immediate_exposure,
        }),
    )
        .into_response()
}

#[derive(Debug, Deserialize)]
pub struct AssessImpactRequest {
    pub affected_service: String,
    pub technical_severity: String,
    pub error_rate: f64,
    pub p95_latency_ms: f64,
    pub confirmed_affected_transactions: Option<u64>,
    pub estimated_customer_exposure: Option<u64>,
    pub estimation_basis: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ImpactMeasure {
    pub status: String,
    pub label: String,
    pub count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub basis: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AssessImpactResponse {
    pub affected_service: String,
    pub business_criticality: String,
    pub critical_journeys: Vec<String>,
    pub critical_processes: Vec<String>,
    pub impact_priority: String,
    pub observed_impact: ImpactMeasure,
    pub estimated_impact: ImpactMeasure,
}

pub async fn assess_business_impact(
    source: Extension<Arc<dyn DependencyImpactSource>>,
    tenant: Extension<TenantContext>,
    Json(req): Json<AssessImpactRequest>,
) -> Response {
    let exposure = match source
        .service_exposure(tenant.tenant_id, req.affected_service.trim())
        .await
    {
        Ok(Some(e)) => e,
        Ok(None) => ServiceBusinessExposure {
            service_name: req.affected_service.trim().to_string(),
            criticality: "unknown".to_string(),
            critical_journeys: Vec::new(),
            critical_processes: Vec::new(),
        },
        Err(_) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": "exposure_source_unavailable" })),
            )
                .into_response();
        }
    };

    let high_symptom = req.technical_severity == "critical"
        || req.error_rate >= 0.05
        || req.p95_latency_ms >= 500.0;
    let has_critical_journey_or_process =
        !exposure.critical_journeys.is_empty() || !exposure.critical_processes.is_empty();

    let impact_priority = match (
        exposure.criticality.as_str(),
        has_critical_journey_or_process,
        high_symptom,
    ) {
        ("critical", _, true) | (_, true, true) => "P1",
        ("critical", _, false) | ("high", _, true) => "P2",
        ("high", _, false) | ("standard", _, true) => "P3",
        _ => "P4",
    }
    .to_string();

    let observed_impact = match req.confirmed_affected_transactions {
        Some(n) => ImpactMeasure {
            status: "confirmed_by_evidence".to_string(),
            label: "Observed (confirmed by evidence)".to_string(),
            count: Some(n),
            basis: None,
        },
        None => ImpactMeasure {
            status: "unknown".to_string(),
            label: "Unknown".to_string(),
            count: None,
            basis: None,
        },
    };

    let estimated_impact = match req.estimated_customer_exposure {
        Some(n) => ImpactMeasure {
            status: "estimated".to_string(),
            label: "Estimate".to_string(),
            count: Some(n),
            basis: Some(
                req.estimation_basis
                    .unwrap_or_else(|| "Extrapolated from historical traffic".to_string()),
            ),
        },
        None => ImpactMeasure {
            status: "unknown".to_string(),
            label: "Unknown".to_string(),
            count: None,
            basis: None,
        },
    };

    (
        StatusCode::OK,
        Json(AssessImpactResponse {
            affected_service: exposure.service_name,
            business_criticality: exposure.criticality,
            critical_journeys: exposure.critical_journeys,
            critical_processes: exposure.critical_processes,
            impact_priority,
            observed_impact,
            estimated_impact,
        }),
    )
        .into_response()
}
