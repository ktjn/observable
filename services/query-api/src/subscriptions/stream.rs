//! `GET /v1/ui/subscriptions/events`: one call registers views and streams their
//! authorized data (PUSH-002, PUSH-007, PUSH-017, PUSH-018, PUSH-020).
//!
//! The per-connection context lives only in this task's stack and the spawned
//! driver's state: session token, authoritative tenant, principal, normalized
//! authorized views, their query/result revisions and a replay ring. Nothing is
//! written anywhere durable, so a disconnect, reload or replica loss simply ends
//! the context and the client reconnects for a fresh authorized snapshot.

use std::collections::HashMap;
use std::convert::Infallible;
use std::time::Duration;

use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::{broadcast, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

use super::events::{Reaction, react};
use super::hub::SubscriptionHub;
use super::protocol::{ChangeOp, ResetReason, SseEvent};
use super::replay::{Cursor, ReplayBuffer, Resume, ResumeKey, StreamId};
use super::views::{ViewDefinition, ViewQuery, parse_views};
use crate::middleware::auth::TenantContext;
use crate::observability::QueryApiMetrics;
use crate::review::{Failure, ReviewProviders};
use crate::traces::AppState;

/// Per-view ceiling on remembered deployment watermarks. Past it, deployment
/// events fall back to an authoritative re-read instead of growing this map.
const MAX_WATERMARK_KEYS: usize = 256;

/// Upper bound on how long the final, best-effort `reset` write may wait for a
/// slot after a connection's queue has overflowed (PUSH-006-C2). The queue is
/// already full, so this is not expected to succeed when the client is truly
/// gone; it must never hang the connection's shutdown.
const OVERFLOW_RESET_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Deserialize)]
pub struct SubscribeParams {
    views: Option<String>,
}

fn error(status: StatusCode, code: &str, message: String) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

/// Why a registered view could not be read.
#[derive(Debug, PartialEq, Eq)]
enum QueryFailure {
    /// The caller may not see this resource.
    Forbidden,
    /// A dependency could not answer; fail closed rather than serve a guess.
    Unavailable,
}

/// Executes one view through the query path that already owns that read. There is
/// no second query engine here: `service-health` goes through the same
/// credential-forwarding provider call `GET /v1/review/services/{id}` uses, and
/// `service-status` through the same catalog-enrichment call
/// `GET /v1/services/summary` uses.
async fn execute(
    view: &ViewQuery,
    state: &AppState,
    providers: &ReviewProviders,
    headers: &HeaderMap,
    tenant_id: Uuid,
) -> Result<Value, QueryFailure> {
    match view {
        ViewQuery::ServiceHealth {
            service_id,
            environment_class,
        } => {
            let outcome = providers
                .service_health(headers, *service_id, environment_class.as_str())
                .await
                .ok_or(QueryFailure::Unavailable)?;
            match outcome {
                Ok(health) => {
                    Ok(serde_json::to_value(&health).map_err(|_| QueryFailure::Unavailable)?)
                }
                Err(Failure::Unauthorized) => Err(QueryFailure::Forbidden),
                Err(Failure::NotFound) | Err(Failure::Unavailable) => {
                    Err(QueryFailure::Unavailable)
                }
            }
        }
        ViewQuery::ServiceStatus { environment } => {
            let enrichment = crate::discovery::fetch_service_catalog_enrichment(
                state,
                tenant_id,
                environment.as_deref(),
            )
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "service-status view could not read catalog enrichment");
                QueryFailure::Unavailable
            })?;
            // `data` *is* the keyed collection, so a pass-through delta's `key`
            // indexes it directly.
            let map: serde_json::Map<String, Value> = enrichment
                .into_iter()
                .map(|(service_name, e)| {
                    (
                        service_name,
                        json!({
                            "active_alert_count": e.active_alert_count,
                            "slo_breaching": e.slo_breaching,
                            "latest_deployment": e.latest_deployment,
                        }),
                    )
                })
                .collect();
            Ok(Value::Object(map))
        }
    }
}

/// A registered view and its live revision state, for this connection only.
struct ViewState {
    def: ViewDefinition,
    result_revision: u64,
    /// Highest `started_at_unix_nano` already applied per service, so a duplicate
    /// or out-of-order deployment marker cannot regress `latest_deployment`.
    /// Dropping a known-superseded event is not a diff: the event itself says it
    /// is older than what this view already shows.
    deployment_watermark: HashMap<String, u64>,
}

impl ViewState {
    fn new(def: ViewDefinition) -> Self {
        Self {
            def,
            result_revision: 0,
            deployment_watermark: HashMap::new(),
        }
    }

    fn next_revision(&mut self) -> u64 {
        self.result_revision += 1;
        self.result_revision
    }

    /// `true` when this deployment marker is newer than what the view already
    /// reflects for that service.
    fn accept_deployment(&mut self, service_name: &str, started_at: u64) -> bool {
        if self.deployment_watermark.len() >= MAX_WATERMARK_KEYS
            && !self.deployment_watermark.contains_key(service_name)
        {
            return false;
        }
        match self.deployment_watermark.get(service_name) {
            Some(seen) if *seen >= started_at => false,
            _ => {
                self.deployment_watermark
                    .insert(service_name.to_string(), started_at);
                true
            }
        }
    }
}

/// Everything the driver task needs. Held in memory for the connection's lifetime.
struct Connection {
    tenant_id: Uuid,
    principal_id: Option<Uuid>,
    session_token: String,
    forwarded: HeaderMap,
    views: Vec<ViewState>,
    stream: StreamId,
    next_seq: u64,
    buffer: ReplayBuffer,
}

impl Connection {
    fn resume_key(&self) -> ResumeKey {
        ResumeKey::new(
            self.tenant_id,
            self.views
                .iter()
                .map(|v| (v.def.view_id.clone(), v.def.query_revision.clone()))
                .collect(),
        )
    }
}

/// Outcome of handing one event to the transport queue.
enum Dispatch {
    Sent,
    /// The fixed-size queue is full: disconnect rather than grow it (PUSH-020).
    Overflowed,
}

fn dispatch(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    conn: &mut Connection,
    metrics: &QueryApiMetrics,
    event: SseEvent,
) -> Dispatch {
    let event = event.bounded();
    let mut sse = Event::default()
        .event(event.name())
        .data(serde_json::to_string(&event.data_json()).unwrap_or_else(|_| "{}".into()));

    if event.is_data() {
        let seq = conn.next_seq;
        conn.next_seq += 1;
        sse = sse.id(conn.stream.event_id(seq));
        conn.buffer.push(seq, event.clone());
    }

    match tx.try_send(Ok(sse)) {
        Ok(()) => {
            metrics
                .subscription_events_total
                .with_label_values(&[event.name()])
                .inc();
            Dispatch::Sent
        }
        Err(_) => Dispatch::Overflowed,
    }
}

/// Makes one bounded, best-effort attempt to deliver a closing `reset` event
/// after the transport queue has overflowed (PUSH-006-C2, PUSH-020). A full
/// queue cannot take one more *queued data* event, but this is a close-out
/// signal rather than one, so it is allowed to wait briefly for a slot instead
/// of failing immediately like `dispatch` does -- and still gives up rather
/// than block the connection's shutdown if no slot frees in time.
async fn dispatch_overflow_reset(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    metrics: &QueryApiMetrics,
) {
    let event = SseEvent::Reset {
        reason: ResetReason::Overflow,
    };
    let sse = Event::default()
        .event(event.name())
        .data(serde_json::to_string(&event.data_json()).unwrap_or_else(|_| "{}".into()));
    if tokio::time::timeout(OVERFLOW_RESET_TIMEOUT, tx.send(Ok(sse)))
        .await
        .is_ok()
    {
        metrics
            .subscription_events_total
            .with_label_values(&[event.name()])
            .inc();
    }
}

/// Re-sends buffered events on a resume, preserving their original ids.
fn dispatch_replayed(
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    conn: &mut Connection,
    metrics: &QueryApiMetrics,
    events: Vec<(u64, SseEvent)>,
) -> Dispatch {
    for (seq, event) in events {
        let sse = Event::default()
            .event(event.name())
            .id(conn.stream.event_id(seq))
            .data(serde_json::to_string(&event.data_json()).unwrap_or_else(|_| "{}".into()));
        conn.buffer.push(seq, event.clone());
        if tx.try_send(Ok(sse)).is_err() {
            return Dispatch::Overflowed;
        }
        metrics
            .subscription_events_total
            .with_label_values(&[event.name()])
            .inc();
    }
    Dispatch::Sent
}

pub async fn subscribe_events(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Extension(hub): Extension<SubscriptionHub>,
    Extension(providers): Extension<ReviewProviders>,
    headers: HeaderMap,
    Query(params): Query<SubscribeParams>,
) -> Response {
    // Browser-only delivery: a native same-origin `EventSource` cannot set request
    // headers, so the credential is always the session cookie. Requiring it here
    // also keeps API-key streaming out, which the first delivery excludes. The
    // token is retained so every batch can be re-authorized (PUSH-007).
    let Some(session_token) = observable_auth::extract_session_cookie(&headers) else {
        return error(
            StatusCode::UNAUTHORIZED,
            "session_required",
            "read-only view subscriptions require a browser session; API-key streaming is not supported"
                .into(),
        );
    };

    let defs = match parse_views(params.views.as_deref()) {
        Ok(defs) => defs,
        Err(e) => return error(StatusCode::BAD_REQUEST, "invalid_request", e.message()),
    };

    let Some(permit) = hub.admit() else {
        tracing::warn!("read-only subscription refused: process connection ceiling reached");
        return error(
            StatusCode::SERVICE_UNAVAILABLE,
            "too_many_connections",
            "this instance is at its subscription connection limit; retry shortly".into(),
        );
    };

    // Observe provider events *before* executing snapshots, so a change committed
    // while a snapshot query runs is still delivered rather than lost in the gap.
    let mut receiver = hub.subscribe();

    let mut forwarded = HeaderMap::new();
    for name in [header::AUTHORIZATION, header::COOKIE] {
        if let Some(value) = headers.get(&name) {
            forwarded.insert(name, value.clone());
        }
    }

    let mut conn = Connection {
        tenant_id: ctx.tenant_id,
        principal_id: ctx.user_id,
        session_token,
        forwarded,
        views: defs.into_iter().map(ViewState::new).collect(),
        stream: StreamId::new(),
        next_seq: 1,
        buffer: ReplayBuffer::default(),
    };

    // Authorize and read every registered view before upgrading to SSE, so a
    // forbidden scope or dependency outage is an ordinary status code rather than
    // an error buried inside an established stream.
    let mut snapshots: Vec<(usize, Value)> = Vec::with_capacity(conn.views.len());
    for (index, view) in conn.views.iter().enumerate() {
        match execute(
            &view.def.query,
            &state,
            &providers,
            &conn.forwarded,
            conn.tenant_id,
        )
        .await
        {
            Ok(data) => snapshots.push((index, data)),
            Err(QueryFailure::Forbidden) => {
                return error(
                    StatusCode::FORBIDDEN,
                    "forbidden",
                    format!(
                        "view {} is not authorized for this session",
                        view.def.view_id
                    ),
                );
            }
            Err(QueryFailure::Unavailable) => {
                return error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "view_unavailable",
                    format!("view {} could not be read", view.def.view_id),
                );
            }
        }
    }

    let cursor = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(Cursor::parse);

    let config = hub.config().clone();
    let (tx, rx) = mpsc::channel::<Result<Event, Infallible>>(config.queue_capacity);
    let metrics = state.metrics.clone();

    // Resume from the retained buffer when continuity can be proven; otherwise say
    // so explicitly with `reset` and re-register with fresh snapshots (PUSH-006).
    let mut resumed = false;
    if let Some(cursor) = cursor {
        match hub.replay().resume(&cursor, &conn.resume_key()) {
            Resume::Replay { events, next_seq } => {
                conn.stream = cursor.stream.clone();
                conn.next_seq = next_seq;
                if matches!(
                    dispatch_replayed(&tx, &mut conn, &metrics, events),
                    Dispatch::Sent
                ) {
                    resumed = true;
                }
            }
            Resume::Reset => {
                dispatch(
                    &tx,
                    &mut conn,
                    &metrics,
                    SseEvent::Reset {
                        reason: ResetReason::CursorUnavailable,
                    },
                );
            }
        }
    }

    if !resumed {
        for (index, data) in snapshots {
            let revision = conn.views[index].next_revision();
            let def = &conn.views[index].def;
            let event = SseEvent::Snapshot {
                view_id: def.view_id.clone(),
                query_revision: def.query_revision.clone(),
                revision,
                data,
            };
            dispatch(&tx, &mut conn, &metrics, event);
        }
    }

    metrics.subscription_connections.inc();
    tracing::info!(
        tenant_id = %conn.tenant_id,
        principal_id = ?conn.principal_id,
        views = conn.views.len(),
        stream = %conn.stream.as_str(),
        "read-only view subscription established"
    );

    let hub_for_task = hub.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let reason = drive(&mut conn, &mut receiver, &tx, &state, &providers, &metrics).await;
        metrics.subscription_connections.dec();
        metrics
            .subscription_disconnects_total
            .with_label_values(&[reason])
            .inc();
        let key = conn.resume_key();
        let Connection {
            stream,
            buffer,
            next_seq,
            ..
        } = conn;
        hub_for_task.replay().retain(&stream, key, buffer, next_seq);
        tracing::info!(stream = %stream.as_str(), reason, "read-only view subscription ended");
    });

    let mut response = Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(config.heartbeat))
        .into_response();
    let out = response.headers_mut();
    // No stored copy, and no intermediary buffering of a long-lived stream.
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    out.insert(
        axum::http::HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    response
}

/// The per-connection event loop. Returns the low-cardinality disconnect reason.
async fn drive(
    conn: &mut Connection,
    receiver: &mut broadcast::Receiver<super::events::ProviderEvent>,
    tx: &mpsc::Sender<Result<Event, Infallible>>,
    state: &AppState,
    providers: &ReviewProviders,
    metrics: &QueryApiMetrics,
) -> &'static str {
    loop {
        // A closed transport queue means the browser went away (tab closed,
        // reload, network drop). Without this arm the task would park on the
        // fan-out forever, holding its connection permit and never handing its
        // replay buffer over for resume.
        let received = tokio::select! {
            biased;
            () = tx.closed() => return "client_disconnected",
            received = receiver.recv() => received,
        };
        let event = match received {
            Ok(event) => event,
            Err(broadcast::error::RecvError::Closed) => return "fanout_closed",
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                // Events were dropped before this connection observed them, so
                // delivery must not be implied complete.
                tracing::warn!(missed, "subscription fell behind the provider fan-out");
                dispatch(
                    tx,
                    conn,
                    metrics,
                    SseEvent::Reset {
                        reason: ResetReason::SubscriberLag,
                    },
                );
                return "subscriber_lag";
            }
        };

        // Which views care, decided against the connection's authoritative tenant.
        let reactions: Vec<(usize, Reaction)> = conn
            .views
            .iter()
            .enumerate()
            .map(|(i, v)| (i, react(&v.def.query, conn.tenant_id, &event)))
            .filter(|(_, r)| *r != Reaction::Ignore)
            .collect();
        if reactions.is_empty() {
            continue;
        }

        // Re-establish the authoritative session before any data batch. A tenant
        // switch, revoked membership or identity outage stops delivery here, even
        // though the connection opened with a then-valid cookie.
        match observable_auth::verify_session(
            &state.http_client,
            &state.auth_service_url,
            &conn.session_token,
        )
        .await
        {
            Ok(session) if session.tenant_id == conn.tenant_id => {}
            Ok(session) => {
                tracing::warn!(
                    from = %conn.tenant_id,
                    to = %session.tenant_id,
                    "session tenant changed; terminating stream before emitting data"
                );
                dispatch(
                    tx,
                    conn,
                    metrics,
                    SseEvent::Reset {
                        reason: ResetReason::AuthorizationChanged,
                    },
                );
                return "tenant_changed";
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    "session could not be re-established; failing closed before emitting data"
                );
                dispatch(
                    tx,
                    conn,
                    metrics,
                    SseEvent::Reset {
                        reason: ResetReason::AuthorizationChanged,
                    },
                );
                return "authorization_failed";
            }
        }

        for (index, reaction) in reactions {
            let emitted = match reaction {
                Reaction::Ignore => continue,
                Reaction::Delta(changes) => {
                    match deployment_delta(&mut conn.views[index], &event, changes) {
                        Some(changes) => {
                            let revision = conn.views[index].next_revision();
                            let def = &conn.views[index].def;
                            Some(SseEvent::Delta {
                                view_id: def.view_id.clone(),
                                query_revision: def.query_revision.clone(),
                                revision,
                                changes,
                            })
                        }
                        None => None,
                    }
                }
                Reaction::Replace => {
                    let query = conn.views[index].def.query.clone();
                    match execute(&query, state, providers, &conn.forwarded, conn.tenant_id).await {
                        Ok(data) => {
                            let revision = conn.views[index].next_revision();
                            let def = &conn.views[index].def;
                            Some(SseEvent::Replace {
                                view_id: def.view_id.clone(),
                                query_revision: def.query_revision.clone(),
                                revision,
                                data,
                            })
                        }
                        Err(QueryFailure::Forbidden) => {
                            tracing::warn!(
                                view = %conn.views[index].def.view_id,
                                "view became unauthorized; terminating stream"
                            );
                            dispatch(
                                tx,
                                conn,
                                metrics,
                                SseEvent::Reset {
                                    reason: ResetReason::AuthorizationChanged,
                                },
                            );
                            return "view_forbidden";
                        }
                        Err(QueryFailure::Unavailable) => {
                            // Never present an unreadable view as current: skip the
                            // update and let the client's staleness state show.
                            tracing::warn!(
                                view = %conn.views[index].def.view_id,
                                "view could not be re-read; no event emitted"
                            );
                            None
                        }
                    }
                }
            };

            if let Some(event) = emitted
                && matches!(dispatch(tx, conn, metrics, event), Dispatch::Overflowed)
            {
                tracing::warn!(
                    stream = %conn.stream.as_str(),
                    "connection queue overflowed; disconnecting"
                );
                dispatch_overflow_reset(tx, metrics).await;
                return "queue_overflow";
            }
        }
    }
}

/// Applies the deployment watermark to a pass-through delta. `None` means the
/// event is a duplicate or older than what this view already shows.
fn deployment_delta(
    view: &mut ViewState,
    event: &super::events::ProviderEvent,
    changes: Vec<ChangeOp>,
) -> Option<Vec<ChangeOp>> {
    let super::events::ProviderEvent::Deployment(deployment) = event else {
        return Some(changes);
    };
    if view.accept_deployment(&deployment.service_name, deployment.started_at_unix_nano) {
        Some(changes)
    } else {
        tracing::debug!(
            service = %deployment.service_name,
            "skipping superseded deployment marker"
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subscriptions::views::EnvironmentClass;

    fn view_state(query: ViewQuery) -> ViewState {
        ViewState::new(ViewDefinition {
            view_id: "v".into(),
            query,
            query_revision: "r1".into(),
        })
    }

    #[test]
    fn result_revisions_increase_monotonically_per_view() {
        let mut view = view_state(ViewQuery::ServiceStatus { environment: None });
        assert_eq!(view.next_revision(), 1);
        assert_eq!(view.next_revision(), 2);
        assert_eq!(view.next_revision(), 3);
    }

    #[test]
    fn a_newer_deployment_marker_is_accepted_once() {
        let mut view = view_state(ViewQuery::ServiceStatus { environment: None });
        assert!(view.accept_deployment("checkout", 100));
        // A duplicate of the same event must not produce a second delta.
        assert!(!view.accept_deployment("checkout", 100));
        assert!(view.accept_deployment("checkout", 200));
    }

    #[test]
    fn an_out_of_order_deployment_marker_cannot_regress_the_view() {
        let mut view = view_state(ViewQuery::ServiceStatus { environment: None });
        assert!(view.accept_deployment("checkout", 200));
        assert!(
            !view.accept_deployment("checkout", 100),
            "an older marker must not overwrite a newer latest_deployment"
        );
    }

    #[test]
    fn watermarks_are_tracked_per_service() {
        let mut view = view_state(ViewQuery::ServiceStatus { environment: None });
        assert!(view.accept_deployment("checkout", 100));
        assert!(view.accept_deployment("payments", 50));
    }

    #[test]
    fn the_watermark_map_is_bounded() {
        let mut view = view_state(ViewQuery::ServiceStatus { environment: None });
        for i in 0..MAX_WATERMARK_KEYS {
            assert!(view.accept_deployment(&format!("svc-{i}"), 1));
        }
        assert_eq!(view.deployment_watermark.len(), MAX_WATERMARK_KEYS);
        // Past the ceiling a new service re-reads authoritatively instead of
        // growing this map without bound.
        assert!(!view.accept_deployment("one-too-many", 1));
        // An already-tracked service still advances.
        assert!(view.accept_deployment("svc-0", 2));
    }

    #[test]
    fn a_resume_key_covers_tenant_and_every_view_query_revision() {
        let conn = Connection {
            tenant_id: Uuid::from_u128(3),
            principal_id: None,
            session_token: "t".into(),
            forwarded: HeaderMap::new(),
            views: vec![
                view_state(ViewQuery::ServiceStatus { environment: None }),
                view_state(ViewQuery::ServiceHealth {
                    service_id: Uuid::from_u128(9),
                    environment_class: EnvironmentClass::Production,
                }),
            ],
            stream: StreamId::new(),
            next_seq: 1,
            buffer: ReplayBuffer::default(),
        };
        let key = conn.resume_key();
        assert_eq!(key.tenant_id, Uuid::from_u128(3));
        assert_eq!(key.views.len(), 2);
    }
}
