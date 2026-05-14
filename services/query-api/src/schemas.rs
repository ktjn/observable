use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use axum::{
    Json,
    extract::{Extension, Path, State},
    http::StatusCode,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const VALID_SIGNAL_TYPES: &[&str] = &[
    "traces",
    "logs",
    "metrics",
    "profiles",
    "events",
    "deployments",
];
const VALID_INTERPRETATION_RULES: &[&str] = &[
    "higher_is_worse",
    "higher_is_better",
    "directional",
    "contextual",
];
const VALID_METRIC_TYPES: &[&str] = &["counter", "gauge", "histogram", "summary"];

// ── Schema entries (global, platform-level) ─────────────────────────────────

#[derive(Serialize, sqlx::FromRow, Debug, Clone, PartialEq)]
pub struct SchemaEntry {
    pub signal_type: String,
    pub field_name: String,
    pub field_type: String,
    pub otel_spec_version: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct SchemaAttributesResponse {
    pub signal_type: String,
    pub attributes: Vec<SchemaEntry>,
}

// ── Semantic annotations (tenant-scoped) ─────────────────────────────────────

#[derive(Serialize, sqlx::FromRow, Debug, Clone, PartialEq)]
pub struct SemanticAnnotation {
    pub tenant_id: Uuid,
    pub signal_type: String,
    pub field_name: String,
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub owner_team: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub known_derivations: Vec<String>,
    pub not_for_billing: bool,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
    pub updated_at: DateTime<Utc>,
}

/// Full-replace annotation request (PUT).
#[derive(Deserialize, Debug, Clone, Default)]
pub struct UpsertAnnotationRequest {
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub owner_team: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub known_derivations: Option<Vec<String>>,
    pub not_for_billing: Option<bool>,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
}

/// Partial-update annotation request (PATCH).
/// Absent (null JSON) fields keep their current value.
/// Use PUT to explicitly clear nullable fields back to NULL.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct PatchAnnotationRequest {
    pub display_name: Option<String>,
    pub business_description: Option<String>,
    pub owner_team: Option<String>,
    pub interpretation_rule: Option<String>,
    pub effective_sample_rate: Option<f64>,
    pub known_derivations: Option<Vec<String>>,
    pub not_for_billing: Option<bool>,
    pub metric_type: Option<String>,
    pub timestamp_column: Option<String>,
    pub unit: Option<String>,
    pub recommended_downsampling: Option<String>,
}

#[derive(Debug)]
pub enum AnnotationError {
    InvalidInput(String),
    Db(sqlx::Error),
}

impl std::fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for AnnotationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Db(e) => Some(e),
            Self::InvalidInput(_) => None,
        }
    }
}

fn validate_signal_type(signal_type: &str) -> Result<(), AnnotationError> {
    if !VALID_SIGNAL_TYPES.contains(&signal_type) {
        return Err(AnnotationError::InvalidInput(format!(
            "signal_type must be one of: {}",
            VALID_SIGNAL_TYPES.join(", ")
        )));
    }
    Ok(())
}

fn validate_upsert(req: &UpsertAnnotationRequest) -> Result<(), AnnotationError> {
    if let Some(rule) = &req.interpretation_rule {
        if !VALID_INTERPRETATION_RULES.contains(&rule.as_str()) {
            return Err(AnnotationError::InvalidInput(format!(
                "interpretation_rule must be one of: {}",
                VALID_INTERPRETATION_RULES.join(", ")
            )));
        }
    }
    if let Some(mt) = &req.metric_type {
        if !VALID_METRIC_TYPES.contains(&mt.as_str()) {
            return Err(AnnotationError::InvalidInput(format!(
                "metric_type must be one of: {}",
                VALID_METRIC_TYPES.join(", ")
            )));
        }
    }
    if let Some(rate) = req.effective_sample_rate {
        if !(0.0..=1.0).contains(&rate) {
            return Err(AnnotationError::InvalidInput(
                "effective_sample_rate must be between 0 and 1".into(),
            ));
        }
    }
    Ok(())
}

fn validate_patch(req: &PatchAnnotationRequest) -> Result<(), AnnotationError> {
    if let Some(rule) = &req.interpretation_rule {
        if !VALID_INTERPRETATION_RULES.contains(&rule.as_str()) {
            return Err(AnnotationError::InvalidInput(format!(
                "interpretation_rule must be one of: {}",
                VALID_INTERPRETATION_RULES.join(", ")
            )));
        }
    }
    if let Some(mt) = &req.metric_type {
        if !VALID_METRIC_TYPES.contains(&mt.as_str()) {
            return Err(AnnotationError::InvalidInput(format!(
                "metric_type must be one of: {}",
                VALID_METRIC_TYPES.join(", ")
            )));
        }
    }
    if let Some(rate) = req.effective_sample_rate {
        if !(0.0..=1.0).contains(&rate) {
            return Err(AnnotationError::InvalidInput(
                "effective_sample_rate must be between 0 and 1".into(),
            ));
        }
    }
    Ok(())
}

// ── Core data-access functions ────────────────────────────────────────────────

pub async fn list_schema_attributes(
    db: &sqlx::PgPool,
    signal_type: &str,
) -> Result<Vec<SchemaEntry>, sqlx::Error> {
    sqlx::query_as::<_, SchemaEntry>(
        "SELECT signal_type, field_name, field_type, otel_spec_version, created_at \
         FROM schema_entries \
         WHERE signal_type = $1 \
         ORDER BY field_name",
    )
    .bind(signal_type)
    .fetch_all(db)
    .await
}

pub async fn get_annotation(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    field_name: &str,
) -> Result<Option<SemanticAnnotation>, sqlx::Error> {
    sqlx::query_as::<_, SemanticAnnotation>(
        "SELECT tenant_id, signal_type, field_name, \
                display_name, business_description, owner_team, \
                interpretation_rule, effective_sample_rate, known_derivations, \
                not_for_billing, metric_type, timestamp_column, unit, \
                recommended_downsampling, updated_at \
         FROM semantic_annotations \
         WHERE tenant_id = $1 AND signal_type = $2 AND field_name = $3",
    )
    .bind(tenant_id)
    .bind(signal_type)
    .bind(field_name)
    .fetch_optional(db)
    .await
}

/// Full-replace upsert. All fields from the request replace existing values.
pub async fn upsert_annotation(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    field_name: &str,
    req: &UpsertAnnotationRequest,
) -> Result<SemanticAnnotation, AnnotationError> {
    validate_signal_type(signal_type)?;
    validate_upsert(req)?;

    let known = req.known_derivations.clone().unwrap_or_default();
    let not_for_billing = req.not_for_billing.unwrap_or(false);

    sqlx::query_as::<_, SemanticAnnotation>(
        "INSERT INTO semantic_annotations \
         (tenant_id, signal_type, field_name, \
          display_name, business_description, owner_team, \
          interpretation_rule, effective_sample_rate, known_derivations, \
          not_for_billing, metric_type, timestamp_column, unit, \
          recommended_downsampling, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, NOW()) \
         ON CONFLICT (tenant_id, signal_type, field_name) DO UPDATE SET \
           display_name             = EXCLUDED.display_name, \
           business_description     = EXCLUDED.business_description, \
           owner_team               = EXCLUDED.owner_team, \
           interpretation_rule      = EXCLUDED.interpretation_rule, \
           effective_sample_rate    = EXCLUDED.effective_sample_rate, \
           known_derivations        = EXCLUDED.known_derivations, \
           not_for_billing          = EXCLUDED.not_for_billing, \
           metric_type              = EXCLUDED.metric_type, \
           timestamp_column         = EXCLUDED.timestamp_column, \
           unit                     = EXCLUDED.unit, \
           recommended_downsampling = EXCLUDED.recommended_downsampling, \
           updated_at               = NOW() \
         RETURNING tenant_id, signal_type, field_name, \
                   display_name, business_description, owner_team, \
                   interpretation_rule, effective_sample_rate, known_derivations, \
                   not_for_billing, metric_type, timestamp_column, unit, \
                   recommended_downsampling, updated_at",
    )
    .bind(tenant_id)
    .bind(signal_type)
    .bind(field_name)
    .bind(&req.display_name)
    .bind(&req.business_description)
    .bind(&req.owner_team)
    .bind(&req.interpretation_rule)
    .bind(req.effective_sample_rate)
    .bind(known.as_slice())
    .bind(not_for_billing)
    .bind(&req.metric_type)
    .bind(&req.timestamp_column)
    .bind(&req.unit)
    .bind(&req.recommended_downsampling)
    .fetch_one(db)
    .await
    .map_err(AnnotationError::Db)
}

/// Partial update. Fields not present in the request keep their current value.
/// To clear a nullable field, use PUT (upsert) with that field set to null.
pub async fn patch_annotation(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    field_name: &str,
    req: &PatchAnnotationRequest,
) -> Result<Option<SemanticAnnotation>, AnnotationError> {
    validate_signal_type(signal_type)?;
    validate_patch(req)?;

    let row = sqlx::query_as::<_, SemanticAnnotation>(
        "UPDATE semantic_annotations SET \
           display_name             = COALESCE($4,  display_name), \
           business_description     = COALESCE($5,  business_description), \
           owner_team               = COALESCE($6,  owner_team), \
           interpretation_rule      = COALESCE($7,  interpretation_rule), \
           effective_sample_rate    = COALESCE($8,  effective_sample_rate), \
           known_derivations        = COALESCE($9,  known_derivations), \
           not_for_billing          = COALESCE($10, not_for_billing), \
           metric_type              = COALESCE($11, metric_type), \
           timestamp_column         = COALESCE($12, timestamp_column), \
           unit                     = COALESCE($13, unit), \
           recommended_downsampling = COALESCE($14, recommended_downsampling), \
           updated_at               = NOW() \
         WHERE tenant_id = $1 AND signal_type = $2 AND field_name = $3 \
         RETURNING tenant_id, signal_type, field_name, \
                   display_name, business_description, owner_team, \
                   interpretation_rule, effective_sample_rate, known_derivations, \
                   not_for_billing, metric_type, timestamp_column, unit, \
                   recommended_downsampling, updated_at",
    )
    .bind(tenant_id)
    .bind(signal_type)
    .bind(field_name)
    .bind(&req.display_name)
    .bind(&req.business_description)
    .bind(&req.owner_team)
    .bind(&req.interpretation_rule)
    .bind(req.effective_sample_rate)
    .bind(req.known_derivations.clone())
    .bind(req.not_for_billing)
    .bind(&req.metric_type)
    .bind(&req.timestamp_column)
    .bind(&req.unit)
    .bind(&req.recommended_downsampling)
    .fetch_optional(db)
    .await
    .map_err(AnnotationError::Db)?;

    Ok(row)
}

/// Returns true if the annotation was found and deleted, false if not found.
pub async fn delete_annotation(
    db: &sqlx::PgPool,
    tenant_id: Uuid,
    signal_type: &str,
    field_name: &str,
) -> Result<bool, sqlx::Error> {
    let deleted: Option<i64> = sqlx::query_scalar(
        "DELETE FROM semantic_annotations \
         WHERE tenant_id = $1 AND signal_type = $2 AND field_name = $3 \
         RETURNING id",
    )
    .bind(tenant_id)
    .bind(signal_type)
    .bind(field_name)
    .fetch_optional(db)
    .await?;
    Ok(deleted.is_some())
}

// ── HTTP handlers ─────────────────────────────────────────────────────────────

pub async fn handle_list_attributes(
    State(state): State<AppState>,
    _ctx: Extension<TenantContext>,
    Path(signal_type): Path<String>,
) -> Result<Json<SchemaAttributesResponse>, StatusCode> {
    if validate_signal_type(&signal_type).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let attributes = list_schema_attributes(&state.db, &signal_type)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to list schema attributes");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(SchemaAttributesResponse {
        signal_type,
        attributes,
    }))
}

pub async fn handle_get_annotation(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((signal_type, field_name)): Path<(String, String)>,
) -> Result<Json<SemanticAnnotation>, StatusCode> {
    if validate_signal_type(&signal_type).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    match get_annotation(&state.db, ctx.tenant_id, &signal_type, &field_name).await {
        Ok(Some(annotation)) => Ok(Json(annotation)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to get annotation");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_upsert_annotation(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((signal_type, field_name)): Path<(String, String)>,
    Json(req): Json<UpsertAnnotationRequest>,
) -> Result<Json<SemanticAnnotation>, StatusCode> {
    match upsert_annotation(&state.db, ctx.tenant_id, &signal_type, &field_name, &req).await {
        Ok(annotation) => Ok(Json(annotation)),
        Err(AnnotationError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid annotation input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(AnnotationError::Db(e)) => {
            tracing::error!(error = %e, "failed to upsert annotation");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_patch_annotation(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((signal_type, field_name)): Path<(String, String)>,
    Json(req): Json<PatchAnnotationRequest>,
) -> Result<Json<SemanticAnnotation>, StatusCode> {
    match patch_annotation(&state.db, ctx.tenant_id, &signal_type, &field_name, &req).await {
        Ok(Some(annotation)) => Ok(Json(annotation)),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(AnnotationError::InvalidInput(msg)) => {
            tracing::warn!(message = %msg, "invalid patch input");
            Err(StatusCode::BAD_REQUEST)
        }
        Err(AnnotationError::Db(e)) => {
            tracing::error!(error = %e, "failed to patch annotation");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn handle_delete_annotation(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Path((signal_type, field_name)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    if validate_signal_type(&signal_type).is_err() {
        return Err(StatusCode::BAD_REQUEST);
    }
    match delete_annotation(&state.db, ctx.tenant_id, &signal_type, &field_name).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "failed to delete annotation");
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_signal_types_are_accepted() {
        for st in VALID_SIGNAL_TYPES {
            assert!(
                validate_signal_type(st).is_ok(),
                "{st} should be a valid signal type"
            );
        }
    }

    #[test]
    fn invalid_signal_type_is_rejected() {
        assert!(validate_signal_type("span").is_err());
        assert!(validate_signal_type("").is_err());
        assert!(validate_signal_type("METRICS").is_err());
    }

    #[test]
    fn valid_interpretation_rules_are_accepted() {
        let req = UpsertAnnotationRequest {
            interpretation_rule: Some("higher_is_worse".into()),
            ..Default::default()
        };
        assert!(validate_upsert(&req).is_ok());
    }

    #[test]
    fn invalid_interpretation_rule_is_rejected() {
        let req = UpsertAnnotationRequest {
            interpretation_rule: Some("bad_rule".into()),
            ..Default::default()
        };
        assert!(validate_upsert(&req).is_err());
    }

    #[test]
    fn valid_metric_types_are_accepted() {
        for mt in VALID_METRIC_TYPES {
            let req = UpsertAnnotationRequest {
                metric_type: Some(mt.to_string()),
                ..Default::default()
            };
            assert!(validate_upsert(&req).is_ok(), "{mt} should be valid");
        }
    }

    #[test]
    fn invalid_metric_type_is_rejected() {
        let req = UpsertAnnotationRequest {
            metric_type: Some("rate".into()),
            ..Default::default()
        };
        assert!(validate_upsert(&req).is_err());
    }

    #[test]
    fn effective_sample_rate_boundaries_are_validated() {
        let valid = UpsertAnnotationRequest {
            effective_sample_rate: Some(0.5),
            ..Default::default()
        };
        assert!(validate_upsert(&valid).is_ok());

        let zero = UpsertAnnotationRequest {
            effective_sample_rate: Some(0.0),
            ..Default::default()
        };
        assert!(validate_upsert(&zero).is_ok());

        let one = UpsertAnnotationRequest {
            effective_sample_rate: Some(1.0),
            ..Default::default()
        };
        assert!(validate_upsert(&one).is_ok());

        let neg = UpsertAnnotationRequest {
            effective_sample_rate: Some(-0.1),
            ..Default::default()
        };
        assert!(validate_upsert(&neg).is_err());

        let over = UpsertAnnotationRequest {
            effective_sample_rate: Some(1.1),
            ..Default::default()
        };
        assert!(validate_upsert(&over).is_err());
    }

    #[test]
    fn semantic_annotation_serializes_expected_shape() {
        let ann = SemanticAnnotation {
            tenant_id: Uuid::nil(),
            signal_type: "metrics".into(),
            field_name: "request_duration_ms".into(),
            display_name: Some("Request Duration (ms)".into()),
            business_description: None,
            owner_team: Some("platform-team".into()),
            interpretation_rule: Some("higher_is_worse".into()),
            effective_sample_rate: Some(0.05),
            known_derivations: vec!["p99_latency".into()],
            not_for_billing: true,
            metric_type: Some("gauge".into()),
            timestamp_column: Some("timestamp_unix_nano".into()),
            unit: Some("ms".into()),
            recommended_downsampling: Some("1m".into()),
            updated_at: chrono::Utc::now(),
        };
        let v = serde_json::to_value(&ann).unwrap();
        assert_eq!(v["signal_type"], "metrics");
        assert_eq!(v["field_name"], "request_duration_ms");
        assert_eq!(v["metric_type"], "gauge");
        assert_eq!(v["unit"], "ms");
        assert_eq!(v["not_for_billing"], true);
        assert_eq!(v["known_derivations"], serde_json::json!(["p99_latency"]));
        assert!(v["business_description"].is_null());
    }

    #[test]
    fn patch_request_defaults_to_all_none() {
        let req = PatchAnnotationRequest::default();
        assert!(req.display_name.is_none());
        assert!(req.metric_type.is_none());
        assert!(req.known_derivations.is_none());
    }
}
