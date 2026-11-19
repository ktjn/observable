//! The SSE data protocol of `GET /v1/ui/subscriptions/events` (PUSH-018).
//!
//! Every data event is versioned (`v`), bounded (`MAX_EVENT_DATA_BYTES`) and tagged
//! with its view ID, query revision and result revision, which is what lets the
//! browser apply events idempotently and discard results belonging to a view
//! definition it has already replaced (PUSH-019).
//!
//! `delta` is deliberately only ever a *pass-through* of an upsert/delete op the
//! domain event already carried; this module offers no diffing primitive, because
//! the server never computes a diff (ADR-046 decision 3).

use serde::Serialize;
use serde_json::{Value, json};

/// Protocol version carried as `v` on every payload.
pub const PROTOCOL_VERSION: u32 = 1;

/// Ceiling on a single event's serialized `data`/`changes` payload. A result that
/// does not fit is downgraded to `invalidate` rather than delivered truncated or
/// unbounded.
pub const MAX_EVENT_DATA_BYTES: usize = 128 * 1024;

/// A trivial keyed change taken verbatim from a domain event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum ChangeOp {
    Upsert { key: String, value: Value },
    Delete { key: String },
}

/// Why the server could not prove continuity and is asking the client to
/// re-register. Reasons are low-cardinality and carry no resource identity, so a
/// reset never discloses the existence of a restricted resource.
///
/// Queue overflow still disconnects (PUSH-020: a full queue cannot carry one
/// more *queued* event) and is recorded as
/// `query_api_subscription_disconnects_total{reason="queue_overflow"}`, but the
/// connection also makes one best-effort, non-blocking attempt to write this
/// reason as a final `reset` frame before closing (PUSH-006-C2): it is a
/// close-out signal, not a queued data event, so writing it does not reopen the
/// unbounded-queue problem PUSH-020 guards against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetReason {
    /// `Last-Event-ID` is outside the retained replay window.
    CursorUnavailable,
    /// This connection fell behind the process-wide provider fan-out, so some
    /// events were dropped before it observed them (PUSH-006).
    SubscriberLag,
    /// Session tenant, membership or role changed, or identity could not be
    /// re-established (PUSH-007). No further data follows.
    AuthorizationChanged,
    /// The connection's fixed-size transport queue overflowed (PUSH-020); the
    /// connection is closing right after this, best-effort frame.
    Overflow,
}

impl ResetReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CursorUnavailable => "cursor_unavailable",
            Self::SubscriberLag => "subscriber_lag",
            Self::AuthorizationChanged => "authorization_changed",
            Self::Overflow => "overflow",
        }
    }
}

/// One SSE event, before it is assigned an `id:` and written to the transport.
#[derive(Debug, Clone, PartialEq)]
pub enum SseEvent {
    /// Full authorized initial result for a view.
    Snapshot {
        view_id: String,
        query_revision: String,
        revision: u64,
        data: Value,
    },
    /// Pass-through of an already-known op from the domain event.
    Delta {
        view_id: String,
        query_revision: String,
        revision: u64,
        changes: Vec<ChangeOp>,
    },
    /// Complete fresh authorized result, re-read through the owning query path.
    Replace {
        view_id: String,
        query_revision: String,
        revision: u64,
        data: Value,
    },
    /// Complete fresh authorized replacement emitted when a replaceable subscriber
    /// lags and intermediate updates are conflated/skipped (PUSH-021).
    ReplaceSuperseding {
        view_id: String,
        query_revision: String,
        revision: u64,
        supersedes_through: u64,
        reason: &'static str,
        data: Value,
    },
    /// Fallback hint: refetch through the normal query API.
    Invalidate {
        view_id: String,
        query_revision: String,
    },
    /// Continuity could not be proven; the client re-registers.
    Reset { reason: ResetReason },
}

impl SseEvent {
    /// The SSE `event:` name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Snapshot { .. } => "snapshot",
            Self::Delta { .. } => "delta",
            Self::Replace { .. } | Self::ReplaceSuperseding { .. } => "replace",
            Self::Invalidate { .. } => "invalidate",
            Self::Reset { .. } => "reset",
        }
    }

    /// `true` for events that carry view data and therefore need an `id:` and a
    /// replay-buffer slot.
    pub fn is_data(&self) -> bool {
        matches!(
            self,
            Self::Snapshot { .. }
                | Self::Delta { .. }
                | Self::Replace { .. }
                | Self::ReplaceSuperseding { .. }
        )
    }

    pub fn view_id(&self) -> Option<&str> {
        match self {
            Self::Snapshot { view_id, .. }
            | Self::Delta { view_id, .. }
            | Self::Replace { view_id, .. }
            | Self::ReplaceSuperseding { view_id, .. }
            | Self::Invalidate { view_id, .. } => Some(view_id),
            Self::Reset { .. } => None,
        }
    }

    pub fn revision(&self) -> Option<u64> {
        match self {
            Self::Snapshot { revision, .. }
            | Self::Delta { revision, .. }
            | Self::Replace { revision, .. }
            | Self::ReplaceSuperseding { revision, .. } => Some(*revision),
            Self::Invalidate { .. } | Self::Reset { .. } => None,
        }
    }

    /// The SSE `data:` payload.
    pub fn data_json(&self) -> Value {
        match self {
            Self::Snapshot {
                view_id,
                query_revision,
                revision,
                data,
            }
            | Self::Replace {
                view_id,
                query_revision,
                revision,
                data,
            } => json!({
                "v": PROTOCOL_VERSION,
                "viewId": view_id,
                "queryRevision": query_revision,
                "revision": revision.to_string(),
                "data": data,
            }),
            Self::ReplaceSuperseding {
                view_id,
                query_revision,
                revision,
                supersedes_through,
                reason,
                data,
            } => json!({
                "v": PROTOCOL_VERSION,
                "viewId": view_id,
                "queryRevision": query_revision,
                "revision": revision.to_string(),
                "supersedesThrough": supersedes_through.to_string(),
                "reason": reason,
                "data": data,
            }),
            Self::Delta {
                view_id,
                query_revision,
                revision,
                changes,
            } => json!({
                "v": PROTOCOL_VERSION,
                "viewId": view_id,
                "queryRevision": query_revision,
                "revision": revision.to_string(),
                "changes": changes,
            }),
            Self::Invalidate {
                view_id,
                query_revision,
            } => json!({
                "v": PROTOCOL_VERSION,
                "viewId": view_id,
                "queryRevision": query_revision,
            }),
            Self::Reset { reason } => json!({
                "v": PROTOCOL_VERSION,
                "reason": reason.as_str(),
            }),
        }
    }

    /// Enforces `MAX_EVENT_DATA_BYTES`: an oversized authorized result becomes an
    /// `invalidate` hint, so the client refetches through the normal query API
    /// instead of the server emitting an unbounded event.
    pub fn bounded(self) -> Self {
        let too_large = serde_json::to_vec(&self.data_json())
            .map(|bytes| bytes.len() > MAX_EVENT_DATA_BYTES)
            .unwrap_or(true);
        if !too_large {
            return self;
        }
        match self {
            Self::Snapshot {
                view_id,
                query_revision,
                ..
            }
            | Self::Delta {
                view_id,
                query_revision,
                ..
            }
            | Self::Replace {
                view_id,
                query_revision,
                ..
            }
            | Self::ReplaceSuperseding {
                view_id,
                query_revision,
                ..
            } => Self::Invalidate {
                view_id,
                query_revision,
            },
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(data: Value) -> SseEvent {
        SseEvent::Snapshot {
            view_id: "health".into(),
            query_revision: "r1".into(),
            revision: 1842,
            data,
        }
    }

    #[test]
    fn a_snapshot_is_tagged_by_view_query_revision_and_result_revision() {
        let payload = snapshot(json!({"service": "payments", "health": "normal"})).data_json();
        assert_eq!(payload["v"], json!(1));
        assert_eq!(payload["viewId"], json!("health"));
        assert_eq!(payload["queryRevision"], json!("r1"));
        assert_eq!(payload["revision"], json!("1842"));
        assert_eq!(payload["data"]["health"], json!("normal"));
    }

    #[test]
    fn a_delta_carries_passed_through_ops_and_no_data_field() {
        let event = SseEvent::Delta {
            view_id: "health".into(),
            query_revision: "r1".into(),
            revision: 1843,
            changes: vec![ChangeOp::Upsert {
                key: "payments".into(),
                value: json!({"health": "degraded"}),
            }],
        };
        assert_eq!(event.name(), "delta");
        let payload = event.data_json();
        assert_eq!(payload["changes"][0]["op"], json!("upsert"));
        assert_eq!(payload["changes"][0]["key"], json!("payments"));
        assert_eq!(payload["changes"][0]["value"]["health"], json!("degraded"));
        assert!(payload.get("data").is_none());
    }

    #[test]
    fn a_delete_op_serializes_without_a_value() {
        let payload = SseEvent::Delta {
            view_id: "v".into(),
            query_revision: "r1".into(),
            revision: 2,
            changes: vec![ChangeOp::Delete { key: "gone".into() }],
        }
        .data_json();
        assert_eq!(payload["changes"][0]["op"], json!("delete"));
        assert!(payload["changes"][0].get("value").is_none());
    }

    #[test]
    fn a_reset_names_a_low_cardinality_reason_and_no_view() {
        let event = SseEvent::Reset {
            reason: ResetReason::CursorUnavailable,
        };
        assert_eq!(event.name(), "reset");
        assert_eq!(event.view_id(), None);
        assert!(!event.is_data());
        assert_eq!(event.data_json()["reason"], json!("cursor_unavailable"));
    }

    #[test]
    fn only_snapshot_delta_and_replace_are_data_events() {
        assert!(snapshot(json!({})).is_data());
        assert!(
            !SseEvent::Invalidate {
                view_id: "v".into(),
                query_revision: "r1".into(),
            }
            .is_data()
        );
    }

    #[test]
    fn an_oversized_result_is_downgraded_to_an_invalidate_hint() {
        let huge = json!({ "blob": "x".repeat(MAX_EVENT_DATA_BYTES + 1) });
        let bounded = snapshot(huge).bounded();
        assert_eq!(bounded.name(), "invalidate");
        assert_eq!(bounded.view_id(), Some("health"));
    }

    #[test]
    fn a_result_within_the_bound_is_left_untouched() {
        let event = snapshot(json!({"health": "normal"}));
        assert_eq!(event.clone().bounded(), event);
    }
}
