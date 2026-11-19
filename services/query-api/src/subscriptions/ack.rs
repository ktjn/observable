//! `POST /v1/ui/subscriptions/{id}/ack`: optional session-bound, batched
//! applied-revision ACKs as advisory browser-processing lag feedback distinct
//! from transport pressure (PUSH-022).
//!
//! ACKs never grant access, never modify domain state, and are never required
//! for SSE delivery or server-side flow control. Every batch is bound to the
//! active authenticated browser session, tenant, principal, stream ID, view ID,
//! query revision, and monotonic emitted revision ceiling.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    Json,
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use super::hub::SubscriptionHub;
use super::views::{MAX_VIEWS_PER_CONNECTION, ViewDefinition};
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;

/// Per-view revision tracking for one live SSE stream.
#[derive(Debug, Clone)]
pub struct ViewAckRecord {
    pub query_revision: String,
    pub highest_emitted_revision: u64,
    pub last_acked_revision: u64,
}

#[derive(Debug, Clone)]
struct ActiveStreamRecord {
    tenant_id: Uuid,
    principal_id: Option<Uuid>,
    views: HashMap<String, ViewAckRecord>,
}

/// Process-local in-memory registry of currently open SSE streams so
/// `POST /v1/ui/subscriptions/{id}/ack` can validate session-bound batched
/// applied-revision ACKs and expose advisory client lag to the stream driver.
#[derive(Clone, Default)]
pub struct AckRegistry {
    inner: Arc<Mutex<HashMap<String, ActiveStreamRecord>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AckValidationError {
    UnknownSubscription,
    Forbidden,
    EmptyBatch,
    BatchTooLarge,
    UnknownView(String),
    StaleQueryRevision(String),
    InvalidRevision(String),
    FutureRevision(String),
    NonmonotonicRevision(String),
}

impl AckValidationError {
    pub fn status(&self) -> StatusCode {
        match self {
            Self::UnknownSubscription => StatusCode::NOT_FOUND,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::EmptyBatch | Self::BatchTooLarge => StatusCode::BAD_REQUEST,
            Self::UnknownView(_)
            | Self::StaleQueryRevision(_)
            | Self::InvalidRevision(_)
            | Self::FutureRevision(_)
            | Self::NonmonotonicRevision(_) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownSubscription => "unknown_subscription",
            Self::Forbidden => "forbidden",
            Self::EmptyBatch => "empty_ack_batch",
            Self::BatchTooLarge => "ack_batch_too_large",
            Self::UnknownView(_) => "unknown_view",
            Self::StaleQueryRevision(_) => "stale_query_revision",
            Self::InvalidRevision(_) => "invalid_revision",
            Self::FutureRevision(_) => "future_revision",
            Self::NonmonotonicRevision(_) => "nonmonotonic_revision",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::UnknownSubscription => {
                "subscription stream is not active on this instance".into()
            }
            Self::Forbidden => {
                "subscription stream belongs to a different tenant or principal".into()
            }
            Self::EmptyBatch => "acks must contain at least one view revision".into(),
            Self::BatchTooLarge => {
                format!("acks batch exceeds maximum of {MAX_VIEWS_PER_CONNECTION} items")
            }
            Self::UnknownView(v) => format!("viewId {v} is not registered on this stream"),
            Self::StaleQueryRevision(v) => {
                format!("queryRevision for viewId {v} does not match the active registration")
            }
            Self::InvalidRevision(v) => {
                format!("revision for viewId {v} must be a positive decimal integer")
            }
            Self::FutureRevision(v) => {
                format!("revision for viewId {v} exceeds the highest emitted revision")
            }
            Self::NonmonotonicRevision(v) => {
                format!("revision for viewId {v} is stale or nonmonotonic")
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ViewAckItem {
    #[serde(rename = "viewId")]
    pub view_id: String,
    #[serde(rename = "queryRevision")]
    pub query_revision: String,
    pub revision: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AckRequest {
    pub acks: Vec<ViewAckItem>,
}

impl AckRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an open stream's initial view definitions.
    pub fn register(
        &self,
        stream_id: &str,
        tenant_id: Uuid,
        principal_id: Option<Uuid>,
        defs: &[ViewDefinition],
    ) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let views = defs
            .iter()
            .map(|d| {
                (
                    d.view_id.clone(),
                    ViewAckRecord {
                        query_revision: d.query_revision.clone(),
                        highest_emitted_revision: 0,
                        last_acked_revision: 0,
                    },
                )
            })
            .collect();
        guard.insert(
            stream_id.to_string(),
            ActiveStreamRecord {
                tenant_id,
                principal_id,
                views,
            },
        );
    }

    /// Removes a stream when its connection closes.
    pub fn unregister(&self, stream_id: &str) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        guard.remove(stream_id);
    }

    /// Records that the server has emitted `revision` for `view_id` on `stream_id`.
    pub fn record_emitted(&self, stream_id: &str, view_id: &str, revision: u64) {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(stream) = guard.get_mut(stream_id)
            && let Some(view) = stream.views.get_mut(view_id)
            && revision > view.highest_emitted_revision
        {
            view.highest_emitted_revision = revision;
        }
    }

    /// Returns the unacknowledged revision lag (`highest_emitted - last_acked`)
    /// when the client has reported at least one ACK for `view_id`, else `0`.
    pub fn advisory_lag(&self, stream_id: &str, view_id: &str) -> u64 {
        let guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let Some(stream) = guard.get(stream_id) else {
            return 0;
        };
        let Some(view) = stream.views.get(view_id) else {
            return 0;
        };
        if view.last_acked_revision == 0 {
            return 0;
        }
        view.highest_emitted_revision
            .saturating_sub(view.last_acked_revision)
    }

    /// Validates and applies a batch of client ACKs atomically: either every
    /// item in the batch is valid and committed, or the batch is rejected with
    /// no state mutation (PUSH-022-C1/C2).
    pub fn apply_batch(
        &self,
        stream_id: &str,
        tenant_id: Uuid,
        principal_id: Option<Uuid>,
        acks: &[ViewAckItem],
    ) -> Result<usize, AckValidationError> {
        if acks.is_empty() {
            return Err(AckValidationError::EmptyBatch);
        }
        if acks.len() > MAX_VIEWS_PER_CONNECTION {
            return Err(AckValidationError::BatchTooLarge);
        }

        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let Some(stream) = guard.get_mut(stream_id) else {
            return Err(AckValidationError::UnknownSubscription);
        };
        if stream.tenant_id != tenant_id {
            return Err(AckValidationError::Forbidden);
        }
        if stream.principal_id.is_some()
            && principal_id.is_some()
            && stream.principal_id != principal_id
        {
            return Err(AckValidationError::Forbidden);
        }

        let mut staged: HashMap<String, u64> = HashMap::with_capacity(acks.len());
        for item in acks {
            let Some(view) = stream.views.get(&item.view_id) else {
                return Err(AckValidationError::UnknownView(item.view_id.clone()));
            };
            if item.query_revision != view.query_revision {
                return Err(AckValidationError::StaleQueryRevision(item.view_id.clone()));
            }
            let parsed = item
                .revision
                .parse::<u64>()
                .ok()
                .filter(|r| *r > 0)
                .ok_or_else(|| AckValidationError::InvalidRevision(item.view_id.clone()))?;
            if parsed > view.highest_emitted_revision {
                return Err(AckValidationError::FutureRevision(item.view_id.clone()));
            }
            let floor = staged
                .get(&item.view_id)
                .copied()
                .unwrap_or(view.last_acked_revision);
            if parsed <= floor {
                return Err(AckValidationError::NonmonotonicRevision(
                    item.view_id.clone(),
                ));
            }
            staged.insert(item.view_id.clone(), parsed);
        }

        for (view_id, rev) in staged {
            if let Some(view) = stream.views.get_mut(&view_id) {
                view.last_acked_revision = rev;
            }
        }
        Ok(acks.len())
    }
}

pub async fn ack_view_revisions(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(hub): Extension<SubscriptionHub>,
    Path(stream_id): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AckRequest>,
) -> Response {
    let Some(session_token) = observable_auth::extract_session_cookie(&headers) else {
        state
            .metrics
            .subscription_acks_total
            .with_label_values(&["rejected"])
            .inc();
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "session_required",
                "message": "read-only view subscription ACKs require an authenticated browser session"
            })),
        )
            .into_response();
    };

    // Re-validate the authoritative session so an expired/revoked session or a
    // tenant switch in another tab fails closed immediately (PUSH-022-C2).
    let verified = match observable_auth::verify_session(
        &state.http_client,
        &state.auth_service_url,
        &session_token,
    )
    .await
    {
        Ok(session) => session,
        Err(_) => {
            state
                .metrics
                .subscription_acks_total
                .with_label_values(&["rejected"])
                .inc();
            return (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "error": "invalid_session",
                    "message": "session could not be validated"
                })),
            )
                .into_response();
        }
    };

    if verified.tenant_id != ctx.tenant_id {
        state
            .metrics
            .subscription_acks_total
            .with_label_values(&["rejected"])
            .inc();
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "forbidden",
                "message": "active session tenant changed"
            })),
        )
            .into_response();
    }

    match hub
        .acks()
        .apply_batch(&stream_id, verified.tenant_id, ctx.user_id, &payload.acks)
    {
        Ok(accepted) => {
            state
                .metrics
                .subscription_acks_total
                .with_label_values(&["accepted"])
                .inc();
            (StatusCode::OK, Json(json!({ "accepted": accepted }))).into_response()
        }
        Err(err) => {
            state
                .metrics
                .subscription_acks_total
                .with_label_values(&["rejected"])
                .inc();
            (
                err.status(),
                Json(json!({
                    "error": err.code(),
                    "message": err.message(),
                })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::views::ViewQuery;

    fn def(view_id: &str, query_rev: &str) -> ViewDefinition {
        ViewDefinition {
            view_id: view_id.into(),
            query: ViewQuery::ServiceStatus { environment: None },
            query_revision: query_rev.into(),
        }
    }

    #[test]
    fn valid_monotonic_batched_acks_advance_last_acked_and_compute_lag() {
        let reg = AckRegistry::new();
        let tenant = Uuid::from_u128(1);
        let user = Some(Uuid::from_u128(10));
        reg.register("s1", tenant, user, &[def("v1", "r1"), def("v2", "r2")]);
        reg.record_emitted("s1", "v1", 5);
        reg.record_emitted("s1", "v2", 3);

        let accepted = reg
            .apply_batch(
                "s1",
                tenant,
                user,
                &[
                    ViewAckItem {
                        view_id: "v1".into(),
                        query_revision: "r1".into(),
                        revision: "2".into(),
                    },
                    ViewAckItem {
                        view_id: "v2".into(),
                        query_revision: "r2".into(),
                        revision: "3".into(),
                    },
                ],
            )
            .unwrap();
        assert_eq!(accepted, 2);
        assert_eq!(reg.advisory_lag("s1", "v1"), 3);
        assert_eq!(reg.advisory_lag("s1", "v2"), 0);
    }

    #[test]
    fn stale_future_nonmonotonic_cross_tenant_and_mismatched_query_revision_acks_are_rejected() {
        let reg = AckRegistry::new();
        let tenant = Uuid::from_u128(1);
        let other_tenant = Uuid::from_u128(2);
        reg.register("s1", tenant, None, &[def("v1", "r1")]);
        reg.record_emitted("s1", "v1", 4);

        // Cross-tenant is forbidden.
        assert_eq!(
            reg.apply_batch(
                "s1",
                other_tenant,
                None,
                &[ViewAckItem {
                    view_id: "v1".into(),
                    query_revision: "r1".into(),
                    revision: "1".into(),
                }]
            )
            .unwrap_err(),
            AckValidationError::Forbidden
        );

        // Future revision (> highest_emitted_revision) is rejected.
        assert_eq!(
            reg.apply_batch(
                "s1",
                tenant,
                None,
                &[ViewAckItem {
                    view_id: "v1".into(),
                    query_revision: "r1".into(),
                    revision: "5".into(),
                }]
            )
            .unwrap_err(),
            AckValidationError::FutureRevision("v1".into())
        );

        // Mismatched query_revision is rejected.
        assert_eq!(
            reg.apply_batch(
                "s1",
                tenant,
                None,
                &[ViewAckItem {
                    view_id: "v1".into(),
                    query_revision: "r-old".into(),
                    revision: "2".into(),
                }]
            )
            .unwrap_err(),
            AckValidationError::StaleQueryRevision("v1".into())
        );

        // Valid ACK to revision 3 succeeds.
        assert_eq!(
            reg.apply_batch(
                "s1",
                tenant,
                None,
                &[ViewAckItem {
                    view_id: "v1".into(),
                    query_revision: "r1".into(),
                    revision: "3".into(),
                }]
            )
            .unwrap(),
            1
        );

        // Duplicate (3) or stale (2) revision is rejected as nonmonotonic.
        for bad in ["3", "2"] {
            assert_eq!(
                reg.apply_batch(
                    "s1",
                    tenant,
                    None,
                    &[ViewAckItem {
                        view_id: "v1".into(),
                        query_revision: "r1".into(),
                        revision: bad.into(),
                    }]
                )
                .unwrap_err(),
                AckValidationError::NonmonotonicRevision("v1".into())
            );
        }
    }
}
