//! Parsing and validation of the `views` query parameter of
//! `GET /v1/ui/subscriptions/events` (PUSH-002).
//!
//! `EventSource` only issues a bodyless `GET`, so the view-definition array travels
//! URL-encoded in the query string (spec/09-api.md). Only bounded, allowlisted typed
//! queries are accepted: an unknown `kind`, a pinned (`fixed`) time range, or more
//! views than `MAX_VIEWS_PER_CONNECTION` is a client error, never a silently narrowed
//! subscription.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use uuid::Uuid;

/// Per-connection view ceiling (PUSH-008: bound fan-out per connection).
pub const MAX_VIEWS_PER_CONNECTION: usize = 16;

/// Upper bound on the raw `views` parameter, so an oversized query string is
/// rejected before any JSON parsing work.
pub const MAX_VIEWS_PARAM_BYTES: usize = 8 * 1024;

/// Why a `views` parameter was refused. Carries the stable machine code the
/// handler reports as `{"error":"invalid_request","message":...}`, matching
/// `review.rs`'s existing error convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewParseError {
    Missing,
    TooLarge,
    Malformed(String),
    Empty,
    TooManyViews,
    DuplicateViewId(String),
    UnknownKind(String),
    PinnedTimeRange,
    InvalidField(String),
}

impl ViewParseError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing => "views query parameter is required".into(),
            Self::TooLarge => {
                format!("views query parameter exceeds {MAX_VIEWS_PARAM_BYTES} bytes")
            }
            Self::Malformed(detail) => {
                format!("views must be a URL-encoded JSON array of view definitions: {detail}")
            }
            Self::Empty => "views must contain at least one view definition".into(),
            Self::TooManyViews => {
                format!("at most {MAX_VIEWS_PER_CONNECTION} views may share one connection")
            }
            Self::DuplicateViewId(id) => format!("duplicate viewId {id}"),
            Self::UnknownKind(kind) => {
                format!("query kind {kind} is not an allowlisted read-only subscription view")
            }
            Self::PinnedTimeRange => {
                "a pinned (fixed) time range does not subscribe; use mode=rolling".into()
            }
            Self::InvalidField(detail) => detail.clone(),
        }
    }
}

/// The two environment classes health assessments are scoped by, mirroring
/// `review.rs`'s `CLASSES`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EnvironmentClass {
    Production,
    NonProduction,
}

impl EnvironmentClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::NonProduction => "non_production",
        }
    }

    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "production" => Some(Self::Production),
            "non_production" => Some(Self::NonProduction),
            _ => None,
        }
    }
}

/// An allowlisted read-only view query. Each variant maps onto a query path this
/// service already owns -- there is no second query engine (ADR-046 decision 4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ViewQuery {
    /// Authorized health assessment of one catalog service in one environment
    /// class, read through the same credential-forwarding provider call
    /// `GET /v1/review/services/{id}` already uses.
    ServiceHealth {
        service_id: Uuid,
        environment_class: EnvironmentClass,
    },
    /// Per-service active alert state and latest deployment/change marker for the
    /// session tenant, read through the same catalog-enrichment call
    /// `GET /v1/services/summary` already uses.
    ServiceStatus { environment: Option<String> },
}

impl ViewQuery {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ServiceHealth { .. } => "service-health",
            Self::ServiceStatus { .. } => "service-status",
        }
    }
}

/// A normalized, authorized-at-registration view definition. `query_revision` is
/// derived from the normalized definition so the browser can discard results for a
/// view definition it has already replaced (PUSH-018/PUSH-019).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewDefinition {
    pub view_id: String,
    pub query: ViewQuery,
    pub query_revision: String,
}

#[derive(Deserialize)]
struct RawView {
    #[serde(rename = "viewId")]
    view_id: String,
    query: Value,
    #[serde(rename = "timeRange")]
    time_range: Option<RawTimeRange>,
    #[allow(dead_code)]
    filters: Option<Value>,
}

#[derive(Deserialize)]
struct RawTimeRange {
    mode: String,
}

/// Parses the raw `views` query-parameter value into normalized definitions.
pub fn parse_views(raw: Option<&str>) -> Result<Vec<ViewDefinition>, ViewParseError> {
    let raw = raw
        .filter(|v| !v.trim().is_empty())
        .ok_or(ViewParseError::Missing)?;
    if raw.len() > MAX_VIEWS_PARAM_BYTES {
        return Err(ViewParseError::TooLarge);
    }

    let parsed: Vec<RawView> =
        serde_json::from_str(raw).map_err(|e| ViewParseError::Malformed(e.to_string()))?;

    if parsed.is_empty() {
        return Err(ViewParseError::Empty);
    }
    if parsed.len() > MAX_VIEWS_PER_CONNECTION {
        return Err(ViewParseError::TooManyViews);
    }

    let mut views = Vec::with_capacity(parsed.len());
    let mut seen: Vec<&str> = Vec::with_capacity(parsed.len());
    for raw_view in &parsed {
        if raw_view.view_id.trim().is_empty() {
            return Err(ViewParseError::InvalidField(
                "viewId must not be empty".into(),
            ));
        }
        if seen.contains(&raw_view.view_id.as_str()) {
            return Err(ViewParseError::DuplicateViewId(raw_view.view_id.clone()));
        }
        seen.push(&raw_view.view_id);

        // A pinned historical range never subscribes (plan section 1); it is a client
        // error rather than a silently live-updated pinned view.
        if let Some(range) = &raw_view.time_range
            && range.mode != "rolling"
        {
            return Err(ViewParseError::PinnedTimeRange);
        }

        let query = parse_query(&raw_view.query)?;
        let query_revision = revision_of(&raw_view.view_id, &query);
        views.push(ViewDefinition {
            view_id: raw_view.view_id.clone(),
            query,
            query_revision,
        });
    }

    Ok(views)
}

fn parse_query(raw: &Value) -> Result<ViewQuery, ViewParseError> {
    let kind = raw
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ViewParseError::InvalidField("query.kind must be a string".into()))?;

    match kind {
        "service-health" => {
            let service_id = raw
                .get("serviceId")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<Uuid>().ok())
                .ok_or_else(|| {
                    ViewParseError::InvalidField(
                        "query.serviceId must be a service UUID for kind service-health".into(),
                    )
                })?;
            let environment_class = raw
                .get("environmentClass")
                .and_then(Value::as_str)
                .and_then(EnvironmentClass::parse)
                .ok_or_else(|| {
                    ViewParseError::InvalidField(
                        "query.environmentClass must be production or non_production".into(),
                    )
                })?;
            Ok(ViewQuery::ServiceHealth {
                service_id,
                environment_class,
            })
        }
        "service-status" => {
            let environment = match raw.get("environment") {
                None | Some(Value::Null) => None,
                Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
                Some(_) => {
                    return Err(ViewParseError::InvalidField(
                        "query.environment must be a non-empty string when present".into(),
                    ));
                }
            };
            Ok(ViewQuery::ServiceStatus { environment })
        }
        other => Err(ViewParseError::UnknownKind(other.to_string())),
    }
}

/// Stable revision of a normalized view definition. Any change to the view's
/// identity or query produces a different revision, which is what lets the client
/// reject results for a replaced view definition.
fn revision_of(view_id: &str, query: &ViewQuery) -> String {
    let normalized = serde_json::to_string(query).unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    view_id.hash(&mut hasher);
    normalized.hash(&mut hasher);
    format!("r{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(json: &str) -> Result<Vec<ViewDefinition>, ViewParseError> {
        parse_views(Some(json))
    }

    #[test]
    fn missing_or_blank_views_parameter_is_rejected() {
        assert_eq!(parse_views(None).unwrap_err(), ViewParseError::Missing);
        assert_eq!(
            parse_views(Some("   ")).unwrap_err(),
            ViewParseError::Missing
        );
    }

    #[test]
    fn non_json_views_parameter_is_malformed() {
        assert!(matches!(
            one("not-json").unwrap_err(),
            ViewParseError::Malformed(_)
        ));
    }

    #[test]
    fn an_unknown_query_kind_is_refused_rather_than_ignored() {
        let err = one(r#"[{"viewId":"v","query":{"kind":"arbitrary-sql"}}]"#).unwrap_err();
        assert_eq!(err, ViewParseError::UnknownKind("arbitrary-sql".into()));
    }

    #[test]
    fn a_pinned_time_range_does_not_subscribe() {
        let err = one(
            r#"[{"viewId":"v","query":{"kind":"service-status"},"timeRange":{"mode":"fixed"}}]"#,
        )
        .unwrap_err();
        assert_eq!(err, ViewParseError::PinnedTimeRange);
    }

    #[test]
    fn service_health_requires_a_service_uuid_and_environment_class() {
        assert!(matches!(
            one(r#"[{"viewId":"v","query":{"kind":"service-health"}}]"#).unwrap_err(),
            ViewParseError::InvalidField(_)
        ));
        let id = Uuid::new_v4();
        assert!(matches!(
            one(&format!(
                r#"[{{"viewId":"v","query":{{"kind":"service-health","serviceId":"{id}","environmentClass":"staging"}}}}]"#
            ))
            .unwrap_err(),
            ViewParseError::InvalidField(_)
        ));
    }

    #[test]
    fn a_valid_rolling_service_health_view_normalizes() {
        let id = Uuid::new_v4();
        let views = one(&format!(
            r#"[{{"viewId":"health","query":{{"kind":"service-health","serviceId":"{id}","environmentClass":"production"}},"timeRange":{{"mode":"rolling","window":"15m"}}}}]"#
        ))
        .unwrap();
        assert_eq!(views.len(), 1);
        assert_eq!(views[0].view_id, "health");
        assert_eq!(
            views[0].query,
            ViewQuery::ServiceHealth {
                service_id: id,
                environment_class: EnvironmentClass::Production,
            }
        );
        assert!(views[0].query_revision.starts_with('r'));
    }

    #[test]
    fn duplicate_view_ids_are_rejected() {
        let err = one(
            r#"[{"viewId":"v","query":{"kind":"service-status"}},{"viewId":"v","query":{"kind":"service-status"}}]"#,
        )
        .unwrap_err();
        assert_eq!(err, ViewParseError::DuplicateViewId("v".into()));
    }

    #[test]
    fn more_views_than_the_per_connection_ceiling_are_rejected() {
        let items: Vec<String> = (0..=MAX_VIEWS_PER_CONNECTION)
            .map(|i| format!(r#"{{"viewId":"v{i}","query":{{"kind":"service-status"}}}}"#))
            .collect();
        let err = one(&format!("[{}]", items.join(","))).unwrap_err();
        assert_eq!(err, ViewParseError::TooManyViews);
    }

    #[test]
    fn differing_view_definitions_get_differing_query_revisions() {
        let a = one(r#"[{"viewId":"v","query":{"kind":"service-status"}}]"#).unwrap();
        let b = one(r#"[{"viewId":"v","query":{"kind":"service-status","environment":"prod"}}]"#)
            .unwrap();
        assert_ne!(a[0].query_revision, b[0].query_revision);
    }

    #[test]
    fn an_identical_view_definition_gets_a_stable_query_revision() {
        let def = r#"[{"viewId":"v","query":{"kind":"service-status","environment":"prod"}}]"#;
        assert_eq!(
            one(def).unwrap()[0].query_revision,
            one(def).unwrap()[0].query_revision
        );
    }
}
