//! AI service observability (`observe-ai-services`, OBS-35-01..08).
//!
//! Read-only views over tenant telemetry in ClickHouse. AI work is recognized by
//! the OpenTelemetry GenAI attributes on spans (`gen_ai.operation.name`,
//! `gen_ai.request.model`, `gen_ai.usage.*`, `gen_ai.tool.name`) and evaluation
//! span events (`gen_ai.evaluation.result`). Attributes Observable adds are
//! namespaced `observable.ai.*`; the full contract is in
//! `docs/ai-service-observability.md` and the capability interface in
//! `spec/business/capabilities.yaml`.
//!
//! Every read is bound to the caller's tenant. Missing evidence is reported as
//! `unknown`, never as a zero or a stable state.

use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use clickhouse::Client;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::middleware::auth::TenantContext;

pub const EVALUATION_EVENT: &str = "gen_ai.evaluation.result";
pub const POLICY_VIOLATION_EVENT: &str = "observable.ai.policy_violation";
pub const GOAL_OUTCOME_EVALUATION: &str = "goal_outcome";

/// State for the AI observability routes: the tenant telemetry store only.
#[derive(Clone)]
pub struct AiObservabilityState {
    pub ch: Client,
}

/// Routes for `observe-ai-services`. Mount behind `require_tenant`; the
/// returned router merges into a router of any state type.
pub fn router<S: Clone + Send + Sync + 'static>(ch: Client) -> Router<S> {
    Router::new()
        .route("/v1/ai/services/{service}/quality", get(get_quality))
        .route(
            "/v1/ai/services/{service}/consumption",
            get(get_consumption),
        )
        .route("/v1/ai/services/{service}/changes", get(get_changes))
        .route(
            "/v1/ai/services/{service}/policy-violations",
            get(get_policy_violations),
        )
        .route(
            "/v1/ai/services/{service}/input-drift",
            get(get_input_drift),
        )
        .route("/v1/ai/services/{service}/feedback", get(get_feedback))
        .route("/v1/ai/interactions/{trace_id}", get(get_interaction))
        .with_state(AiObservabilityState { ch })
}

fn nanos(t: &DateTime<Utc>) -> u64 {
    t.timestamp_nanos_opt().unwrap_or(0).max(0) as u64
}

fn internal(e: clickhouse::error::Error) -> StatusCode {
    tracing::error!(error = %e, "ai observability query failed");
    StatusCode::INTERNAL_SERVER_ERROR
}

fn check_window(from: &DateTime<Utc>, to: &DateTime<Utc>) -> Result<(), StatusCode> {
    if from < to {
        Ok(())
    } else {
        Err(StatusCode::BAD_REQUEST)
    }
}

/// A baseline window and a current window, both half-open `[from, to)`.
#[derive(Debug, Deserialize)]
pub struct CompareWindows {
    pub baseline_from: DateTime<Utc>,
    pub baseline_to: DateTime<Utc>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

impl CompareWindows {
    fn validate(&self) -> Result<(), StatusCode> {
        check_window(&self.baseline_from, &self.baseline_to)?;
        check_window(&self.from, &self.to)
    }
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AssessmentState {
    /// The current window breaches the agreed bound against the baseline.
    Degraded,
    /// Both windows have evidence and the bound is not breached.
    Stable,
    /// A window has no evidence; nothing is inferred.
    Unknown,
}

// ── OBS-35-01 quality ───────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct QualityQuery {
    pub evaluation: String,
    #[serde(flatten)]
    pub windows: CompareWindows,
    /// Largest acceptable drop of the mean score (absolute). Default 0.1.
    pub max_drop: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ScoreWindow {
    pub mean_score: f64,
    pub evaluations: u64,
}

#[derive(Debug, Serialize)]
pub struct QualityResponse {
    pub service: String,
    pub evaluation: String,
    pub baseline: Option<ScoreWindow>,
    pub current: Option<ScoreWindow>,
    pub drop: Option<f64>,
    pub max_drop: f64,
    pub state: AssessmentState,
}

/// Compares mean evaluation scores; a missing window is `unknown`.
pub fn assess_quality(
    baseline: Option<&ScoreWindow>,
    current: Option<&ScoreWindow>,
    max_drop: f64,
) -> (Option<f64>, AssessmentState) {
    match (baseline, current) {
        (Some(b), Some(c)) => {
            let drop = b.mean_score - c.mean_score;
            let state = if drop > max_drop {
                AssessmentState::Degraded
            } else {
                AssessmentState::Stable
            };
            (Some(drop), state)
        }
        _ => (None, AssessmentState::Unknown),
    }
}

fn score_window(sum: f64, n: u64) -> Option<ScoreWindow> {
    (n > 0).then(|| ScoreWindow {
        mean_score: sum / n as f64,
        evaluations: n,
    })
}

#[derive(clickhouse::Row, Deserialize)]
struct WindowSumsRow {
    base_sum: f64,
    base_n: u64,
    cur_sum: f64,
    cur_n: u64,
}

pub async fn get_quality(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<QualityQuery>,
) -> Result<Json<QualityResponse>, StatusCode> {
    q.windows.validate()?;
    let max_drop = q.max_drop.unwrap_or(0.1);
    if !(max_drop.is_finite() && max_drop >= 0.0) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let w = &q.windows;
    let row: WindowSumsRow = state
        .ch
        .query(
            "SELECT \
               sumIf(score, ts >= ? AND ts < ?) AS base_sum, countIf(ts >= ? AND ts < ?) AS base_n, \
               sumIf(score, ts >= ? AND ts < ?) AS cur_sum, countIf(ts >= ? AND ts < ?) AS cur_n \
             FROM ( \
               SELECT e.timestamp_unix_nano AS ts, \
                      JSONExtractFloat(e.attributes, 'gen_ai.evaluation.score.value') AS score \
               FROM observable.span_events AS e \
               INNER JOIN observable.spans AS s \
                 ON e.tenant_id = s.tenant_id AND e.trace_id = s.trace_id AND e.span_id = s.span_id \
               WHERE e.tenant_id = ? AND s.tenant_id = ? AND s.service_name = ? \
                 AND e.name = ? \
                 AND JSONExtractString(e.attributes, 'gen_ai.evaluation.name') = ? \
                 AND JSONHas(e.attributes, 'gen_ai.evaluation.score.value') \
             )",
        )
        .bind(nanos(&w.baseline_from))
        .bind(nanos(&w.baseline_to))
        .bind(nanos(&w.baseline_from))
        .bind(nanos(&w.baseline_to))
        .bind(nanos(&w.from))
        .bind(nanos(&w.to))
        .bind(nanos(&w.from))
        .bind(nanos(&w.to))
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(&service)
        .bind(EVALUATION_EVENT)
        .bind(&q.evaluation)
        .fetch_one()
        .await
        .map_err(internal)?;
    let baseline = score_window(row.base_sum, row.base_n);
    let current = score_window(row.cur_sum, row.cur_n);
    let (drop, state) = assess_quality(baseline.as_ref(), current.as_ref(), max_drop);
    Ok(Json(QualityResponse {
        service,
        evaluation: q.evaluation,
        baseline,
        current,
        drop,
        max_drop,
        state,
    }))
}

// ── OBS-35-02 consumption ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ConsumptionQuery {
    #[serde(flatten)]
    pub windows: CompareWindows,
    /// Current tokens above `baseline_tokens * spike_multiplier` is a spike. Default 3.
    pub spike_multiplier: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpikeState {
    Spike,
    Normal,
    /// No baseline consumption to compare against.
    NoBaseline,
}

#[derive(Debug, Serialize)]
pub struct FeatureConsumption {
    pub feature: String,
    pub owner: Option<String>,
    pub calls: u64,
    pub failed_calls: u64,
    pub p95_duration_ms: Option<f64>,
    pub tokens: u64,
    pub baseline_tokens: u64,
    pub cost: f64,
    pub spike: SpikeState,
}

#[derive(Debug, Serialize)]
pub struct ConsumptionResponse {
    pub service: String,
    pub spike_multiplier: f64,
    pub features: Vec<FeatureConsumption>,
}

pub fn classify_spike(tokens: u64, baseline_tokens: u64, multiplier: f64) -> SpikeState {
    if baseline_tokens == 0 {
        SpikeState::NoBaseline
    } else if tokens as f64 > baseline_tokens as f64 * multiplier {
        SpikeState::Spike
    } else {
        SpikeState::Normal
    }
}

#[derive(clickhouse::Row, Deserialize)]
struct ConsumptionRow {
    feature: String,
    owner: String,
    calls: u64,
    failed_calls: u64,
    p95_ns: f64,
    tokens: u64,
    baseline_tokens: u64,
    cost: f64,
}

pub async fn get_consumption(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<ConsumptionQuery>,
) -> Result<Json<ConsumptionResponse>, StatusCode> {
    q.windows.validate()?;
    let multiplier = q.spike_multiplier.unwrap_or(3.0);
    if !(multiplier.is_finite() && multiplier >= 1.0) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let w = &q.windows;
    let rows: Vec<ConsumptionRow> = state
        .ch
        .query(
            "SELECT feature, owner, \
               countIf(cur) AS calls, \
               countIf(cur AND status_code = 'ERROR') AS failed_calls, \
               if(countIf(cur) = 0, nan, quantileIf(0.95)(toFloat64(duration_ns), cur)) AS p95_ns, \
               sumIf(span_tokens, cur) AS tokens, \
               sumIf(span_tokens, base) AS baseline_tokens, \
               sumIf(span_cost, cur) AS cost \
             FROM ( \
               SELECT \
                 if(JSONExtractString(attributes, 'observable.ai.feature') = '', 'unattributed', \
                    JSONExtractString(attributes, 'observable.ai.feature')) AS feature, \
                 JSONExtractString(attributes, 'observable.ai.feature.owner') AS owner, \
                 status_code, duration_ns, \
                 JSONExtractUInt(attributes, 'gen_ai.usage.input_tokens') \
                   + JSONExtractUInt(attributes, 'gen_ai.usage.output_tokens') AS span_tokens, \
                 JSONExtractFloat(attributes, 'observable.ai.usage.cost') AS span_cost, \
                 start_time_unix_nano >= ? AND start_time_unix_nano < ? AS cur, \
                 start_time_unix_nano >= ? AND start_time_unix_nano < ? AS base \
               FROM observable.spans \
               WHERE tenant_id = ? AND service_name = ? \
                 AND JSONExtractString(attributes, 'gen_ai.operation.name') != '' \
             ) \
             WHERE cur OR base \
             GROUP BY feature, owner \
             ORDER BY tokens DESC, feature",
        )
        .bind(nanos(&w.from))
        .bind(nanos(&w.to))
        .bind(nanos(&w.baseline_from))
        .bind(nanos(&w.baseline_to))
        .bind(ctx.tenant_id)
        .bind(&service)
        .fetch_all()
        .await
        .map_err(internal)?;
    let features = rows
        .into_iter()
        .map(|r| FeatureConsumption {
            spike: classify_spike(r.tokens, r.baseline_tokens, multiplier),
            feature: r.feature,
            owner: (!r.owner.is_empty()).then_some(r.owner),
            calls: r.calls,
            failed_calls: r.failed_calls,
            p95_duration_ms: r.p95_ns.is_finite().then(|| r.p95_ns / 1e6),
            tokens: r.tokens,
            baseline_tokens: r.baseline_tokens,
            cost: r.cost,
        })
        .collect();
    Ok(Json(ConsumptionResponse {
        service,
        spike_multiplier: multiplier,
        features,
    }))
}

// ── OBS-35-03 model and instruction changes ─────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChangesQuery {
    pub evaluation: String,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ConfigurationRevision {
    pub model: String,
    pub instruction_version: String,
    pub first_seen: DateTime<Utc>,
    pub mean_score: Option<f64>,
    pub evaluations: u64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ConfigurationChange {
    pub at: DateTime<Utc>,
    pub from_model: String,
    pub to_model: String,
    pub from_instruction_version: String,
    pub to_instruction_version: String,
    pub quality_before: Option<f64>,
    pub quality_after: Option<f64>,
    pub quality_delta: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ChangesResponse {
    pub service: String,
    pub evaluation: String,
    pub revisions: Vec<ConfigurationRevision>,
    pub changes: Vec<ConfigurationChange>,
}

/// Each consecutive pair of revisions (ordered by first seen) is one change.
pub fn derive_changes(revisions: &[ConfigurationRevision]) -> Vec<ConfigurationChange> {
    revisions
        .windows(2)
        .map(|pair| {
            let (a, b) = (&pair[0], &pair[1]);
            ConfigurationChange {
                at: b.first_seen,
                from_model: a.model.clone(),
                to_model: b.model.clone(),
                from_instruction_version: a.instruction_version.clone(),
                to_instruction_version: b.instruction_version.clone(),
                quality_before: a.mean_score,
                quality_after: b.mean_score,
                quality_delta: a.mean_score.zip(b.mean_score).map(|(x, y)| y - x),
            }
        })
        .collect()
}

#[derive(clickhouse::Row, Deserialize)]
struct RevisionRow {
    model: String,
    instruction_version: String,
    first_seen: u64,
    score_sum: f64,
    evaluations: u64,
}

pub async fn get_changes(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<ChangesQuery>,
) -> Result<Json<ChangesResponse>, StatusCode> {
    check_window(&q.from, &q.to)?;
    let rows: Vec<RevisionRow> = state
        .ch
        .query(
            "SELECT s.model AS model, s.instruction_version AS instruction_version, \
               min(s.start_time_unix_nano) AS first_seen, \
               sumIf(JSONExtractFloat(e.attributes, 'gen_ai.evaluation.score.value'), e.name != '') AS score_sum, \
               countIf(e.name != '') AS evaluations \
             FROM ( \
               SELECT tenant_id, trace_id, span_id, start_time_unix_nano, \
                 JSONExtractString(attributes, 'gen_ai.request.model') AS model, \
                 JSONExtractString(attributes, 'observable.ai.instruction.version') AS instruction_version \
               FROM observable.spans \
               WHERE tenant_id = ? AND service_name = ? \
                 AND start_time_unix_nano >= ? AND start_time_unix_nano < ? \
                 AND JSONExtractString(attributes, 'gen_ai.operation.name') != '' \
             ) AS s \
             LEFT JOIN ( \
               SELECT tenant_id, trace_id, span_id, name, attributes \
               FROM observable.span_events \
               WHERE tenant_id = ? AND name = ? \
                 AND JSONExtractString(attributes, 'gen_ai.evaluation.name') = ? \
             ) AS e \
               ON e.tenant_id = s.tenant_id AND e.trace_id = s.trace_id AND e.span_id = s.span_id \
             GROUP BY model, instruction_version \
             ORDER BY first_seen",
        )
        .bind(ctx.tenant_id)
        .bind(&service)
        .bind(nanos(&q.from))
        .bind(nanos(&q.to))
        .bind(ctx.tenant_id)
        .bind(EVALUATION_EVENT)
        .bind(&q.evaluation)
        .fetch_all()
        .await
        .map_err(internal)?;
    let revisions: Vec<ConfigurationRevision> = rows
        .into_iter()
        .map(|r| ConfigurationRevision {
            model: r.model,
            instruction_version: r.instruction_version,
            first_seen: DateTime::from_timestamp_nanos(r.first_seen as i64),
            mean_score: (r.evaluations > 0).then(|| r.score_sum / r.evaluations as f64),
            evaluations: r.evaluations,
        })
        .collect();
    let changes = derive_changes(&revisions);
    Ok(Json(ChangesResponse {
        service,
        evaluation: q.evaluation,
        revisions,
        changes,
    }))
}

// ── OBS-35-05 policy violations ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WindowQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct PolicyViolation {
    pub at: DateTime<Utc>,
    pub trace_id: String,
    pub span_id: String,
    pub kind: String,
    pub feature: String,
    /// Set when the violation came from a controlled test (safety probe).
    pub safety_probe_id: Option<String>,
    pub owner: Option<String>,
    /// True only when an owner is known to flag it to.
    pub flagged_to_owner: bool,
}

#[derive(Debug, Serialize)]
pub struct PolicyViolationsResponse {
    pub service: String,
    pub violations: Vec<PolicyViolation>,
    /// Violations with no owner to flag them to.
    pub unowned: u64,
}

#[derive(clickhouse::Row, Deserialize)]
struct ViolationRow {
    ts: u64,
    trace_id: String,
    span_id: String,
    kind: String,
    probe: String,
    feature: String,
    owner: String,
}

pub async fn get_policy_violations(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<PolicyViolationsResponse>, StatusCode> {
    check_window(&q.from, &q.to)?;
    let rows: Vec<ViolationRow> = state
        .ch
        .query(
            "SELECT e.timestamp_unix_nano AS ts, e.trace_id AS trace_id, e.span_id AS span_id, \
               JSONExtractString(e.attributes, 'observable.ai.policy.kind') AS kind, \
               JSONExtractString(e.attributes, 'observable.ai.safety_probe.id') AS probe, \
               JSONExtractString(s.attributes, 'observable.ai.feature') AS feature, \
               JSONExtractString(s.attributes, 'observable.ai.feature.owner') AS owner \
             FROM observable.span_events AS e \
             INNER JOIN observable.spans AS s \
               ON e.tenant_id = s.tenant_id AND e.trace_id = s.trace_id AND e.span_id = s.span_id \
             WHERE e.tenant_id = ? AND s.tenant_id = ? AND s.service_name = ? AND e.name = ? \
               AND e.timestamp_unix_nano >= ? AND e.timestamp_unix_nano < ? \
             ORDER BY ts DESC \
             LIMIT 1000",
        )
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(&service)
        .bind(POLICY_VIOLATION_EVENT)
        .bind(nanos(&q.from))
        .bind(nanos(&q.to))
        .fetch_all()
        .await
        .map_err(internal)?;
    let violations: Vec<PolicyViolation> = rows
        .into_iter()
        .map(|r| {
            let owner = (!r.owner.is_empty()).then_some(r.owner);
            PolicyViolation {
                at: DateTime::from_timestamp_nanos(r.ts as i64),
                trace_id: r.trace_id,
                span_id: r.span_id,
                kind: if r.kind.is_empty() {
                    "unspecified".into()
                } else {
                    r.kind
                },
                feature: r.feature,
                safety_probe_id: (!r.probe.is_empty()).then_some(r.probe),
                flagged_to_owner: owner.is_some(),
                owner,
            }
        })
        .collect();
    let unowned = violations.iter().filter(|v| !v.flagged_to_owner).count() as u64;
    Ok(Json(PolicyViolationsResponse {
        service,
        violations,
        unowned,
    }))
}

// ── OBS-35-06 input drift ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct DriftQuery {
    #[serde(flatten)]
    pub windows: CompareWindows,
    /// Absolute change of a topic's share that needs review. Default 0.1.
    pub threshold: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TopicShare {
    pub topic: String,
    pub baseline_share: f64,
    pub current_share: f64,
    pub shift: f64,
    pub review_required: bool,
}

#[derive(Debug, Serialize)]
pub struct DriftResponse {
    pub service: String,
    pub threshold: f64,
    pub baseline_interactions: u64,
    pub current_interactions: u64,
    pub topics: Vec<TopicShare>,
    pub state: AssessmentState,
}

/// Topic shares per window; a shift at or above `threshold` needs review.
pub fn topic_shifts(
    counts: &[(String, u64, u64)],
    threshold: f64,
) -> (u64, u64, Vec<TopicShare>, AssessmentState) {
    let base_total: u64 = counts.iter().map(|c| c.1).sum();
    let cur_total: u64 = counts.iter().map(|c| c.2).sum();
    if base_total == 0 || cur_total == 0 {
        return (base_total, cur_total, Vec::new(), AssessmentState::Unknown);
    }
    let mut topics: Vec<TopicShare> = counts
        .iter()
        .map(|(topic, b, c)| {
            let baseline_share = *b as f64 / base_total as f64;
            let current_share = *c as f64 / cur_total as f64;
            let shift = current_share - baseline_share;
            TopicShare {
                topic: topic.clone(),
                baseline_share,
                current_share,
                shift,
                review_required: shift.abs() >= threshold,
            }
        })
        .collect();
    topics.sort_by(|a, b| b.shift.abs().total_cmp(&a.shift.abs()));
    let state = if topics.iter().any(|t| t.review_required) {
        AssessmentState::Degraded
    } else {
        AssessmentState::Stable
    };
    (base_total, cur_total, topics, state)
}

#[derive(clickhouse::Row, Deserialize)]
struct TopicRow {
    topic: String,
    base_n: u64,
    cur_n: u64,
}

pub async fn get_input_drift(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<DriftQuery>,
) -> Result<Json<DriftResponse>, StatusCode> {
    q.windows.validate()?;
    let threshold = q.threshold.unwrap_or(0.1);
    if !(threshold.is_finite() && threshold > 0.0 && threshold <= 1.0) {
        return Err(StatusCode::BAD_REQUEST);
    }
    let w = &q.windows;
    let rows: Vec<TopicRow> = state
        .ch
        .query(
            "SELECT topic, countIf(base) AS base_n, countIf(cur) AS cur_n \
             FROM ( \
               SELECT JSONExtractString(attributes, 'observable.ai.input.topic') AS topic, \
                 start_time_unix_nano >= ? AND start_time_unix_nano < ? AS base, \
                 start_time_unix_nano >= ? AND start_time_unix_nano < ? AS cur \
               FROM observable.spans \
               WHERE tenant_id = ? AND service_name = ? \
                 AND JSONExtractString(attributes, 'observable.ai.input.topic') != '' \
             ) \
             WHERE base OR cur \
             GROUP BY topic",
        )
        .bind(nanos(&w.baseline_from))
        .bind(nanos(&w.baseline_to))
        .bind(nanos(&w.from))
        .bind(nanos(&w.to))
        .bind(ctx.tenant_id)
        .bind(&service)
        .fetch_all()
        .await
        .map_err(internal)?;
    let counts: Vec<(String, u64, u64)> = rows
        .into_iter()
        .map(|r| (r.topic, r.base_n, r.cur_n))
        .collect();
    let (baseline_interactions, current_interactions, topics, state) =
        topic_shifts(&counts, threshold);
    Ok(Json(DriftResponse {
        service,
        threshold,
        baseline_interactions,
        current_interactions,
        topics,
        state,
    }))
}

// ── OBS-35-07 feedback ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FeedbackQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub sentiment: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FeedbackLink {
    pub feedback_id: Uuid,
    pub at: DateTime<Utc>,
    pub sentiment: String,
    pub comment: String,
    /// The interaction the feedback refers to; `None` when the feedback carries no trace.
    pub interaction_trace_id: Option<String>,
    /// Whether that interaction's AI spans are still retained for this tenant.
    pub interaction_found: bool,
}

#[derive(Debug, Serialize)]
pub struct FeedbackResponse {
    pub service: String,
    pub feedback: Vec<FeedbackLink>,
}

#[derive(clickhouse::Row, Deserialize)]
struct FeedbackRow {
    #[serde(with = "clickhouse::serde::uuid")]
    log_id: Uuid,
    ts: u64,
    sentiment: String,
    body: String,
    trace_id: String,
    found: u8,
}

pub async fn get_feedback(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(service): Path<String>,
    Query(q): Query<FeedbackQuery>,
) -> Result<Json<FeedbackResponse>, StatusCode> {
    check_window(&q.from, &q.to)?;
    let sentiment = q.sentiment.unwrap_or_default();
    let rows: Vec<FeedbackRow> = state
        .ch
        .query(
            "SELECT l.log_id AS log_id, l.timestamp_unix_nano AS ts, \
               JSONExtractString(l.attributes, 'observable.ai.feedback.sentiment') AS sentiment, \
               l.body AS body, ifNull(l.trace_id, '') AS trace_id, \
               ifNull(l.trace_id, '') != '' AND ifNull(l.trace_id, '') IN ( \
                 SELECT trace_id FROM observable.spans \
                 WHERE tenant_id = ? \
                   AND JSONExtractString(attributes, 'gen_ai.operation.name') != '' \
               ) AS found \
             FROM observable.logs AS l \
             WHERE l.tenant_id = ? AND l.service_name = ? \
               AND l.timestamp_unix_nano >= ? AND l.timestamp_unix_nano < ? \
               AND JSONExtractString(l.attributes, 'observable.ai.feedback.sentiment') != '' \
               AND (? = '' OR JSONExtractString(l.attributes, 'observable.ai.feedback.sentiment') = ?) \
             ORDER BY ts DESC \
             LIMIT 1000",
        )
        .bind(ctx.tenant_id)
        .bind(ctx.tenant_id)
        .bind(&service)
        .bind(nanos(&q.from))
        .bind(nanos(&q.to))
        .bind(&sentiment)
        .bind(&sentiment)
        .fetch_all()
        .await
        .map_err(internal)?;
    let feedback = rows
        .into_iter()
        .map(|r| FeedbackLink {
            feedback_id: r.log_id,
            at: DateTime::from_timestamp_nanos(r.ts as i64),
            sentiment: r.sentiment,
            comment: r.body,
            interaction_trace_id: (!r.trace_id.is_empty()).then_some(r.trace_id),
            interaction_found: r.found == 1,
        })
        .collect();
    Ok(Json(FeedbackResponse { service, feedback }))
}

// ── OBS-35-04 / OBS-35-08 interaction reconstruction ────────────────────────

#[derive(Debug, Serialize)]
pub struct InteractionStep {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub service: String,
    pub operation: String,
    pub name: String,
    pub started_at: DateTime<Utc>,
    pub duration_ms: f64,
    pub status: String,
    pub model: Option<String>,
    pub tool: Option<String>,
    /// `gen_ai.*` and `observable.ai.*` attributes only (scrubbed at normalization).
    pub attributes: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GoalOutcome {
    Succeeded,
    Failed,
    /// No goal evaluation was recorded; tool-call results are not used instead.
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct Evaluation {
    pub span_id: String,
    pub name: String,
    pub score: Option<f64>,
    pub label: Option<String>,
    /// Who or what produced the evaluation (provenance).
    pub evaluator: Option<String>,
    pub explanation: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InteractionResponse {
    pub trace_id: String,
    pub steps: Vec<InteractionStep>,
    pub tool_calls: u64,
    pub failed_tool_calls: u64,
    pub goal_outcome: GoalOutcome,
    pub goal_evaluator: Option<String>,
    pub evaluations: Vec<Evaluation>,
}

/// The goal outcome comes only from a `goal_outcome` evaluation; the latest one wins.
/// Tool-call success never implies goal success.
pub fn goal_outcome(evaluations: &[Evaluation]) -> (GoalOutcome, Option<String>) {
    match evaluations
        .iter()
        .rev()
        .find(|e| e.name == GOAL_OUTCOME_EVALUATION)
    {
        None => (GoalOutcome::Unknown, None),
        Some(e) => {
            let outcome = match e.label.as_deref() {
                Some("succeeded" | "achieved" | "pass") => GoalOutcome::Succeeded,
                Some("failed" | "not_achieved" | "fail") => GoalOutcome::Failed,
                _ => GoalOutcome::Unknown,
            };
            (outcome, e.evaluator.clone())
        }
    }
}

fn ai_attributes(raw: &str) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Object(map)) => map
            .into_iter()
            .filter(|(k, _)| k.starts_with("gen_ai.") || k.starts_with("observable.ai."))
            .collect(),
        _ => serde_json::Map::new(),
    }
}

#[derive(clickhouse::Row, Deserialize)]
struct StepRow {
    span_id: String,
    parent_span_id: Option<String>,
    service_name: String,
    operation_name: String,
    start_time_unix_nano: u64,
    duration_ns: u64,
    status_code: String,
    attributes: String,
}

#[derive(clickhouse::Row, Deserialize)]
struct EvaluationRow {
    span_id: String,
    attributes: String,
}

pub async fn get_interaction(
    State(state): State<AiObservabilityState>,
    Extension(ctx): Extension<TenantContext>,
    Path(trace_id): Path<String>,
) -> Result<Json<InteractionResponse>, StatusCode> {
    let rows: Vec<StepRow> = state
        .ch
        .query(
            "SELECT span_id, parent_span_id, service_name, operation_name, start_time_unix_nano, \
               duration_ns, status_code, attributes \
             FROM observable.spans \
             WHERE tenant_id = ? AND trace_id = ? \
               AND JSONExtractString(attributes, 'gen_ai.operation.name') != '' \
             ORDER BY start_time_unix_nano, span_id",
        )
        .bind(ctx.tenant_id)
        .bind(&trace_id)
        .fetch_all()
        .await
        .map_err(internal)?;
    // Unknown trace, another tenant's trace and a trace without AI work look the same.
    if rows.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let eval_rows: Vec<EvaluationRow> = state
        .ch
        .query(
            "SELECT span_id, attributes FROM observable.span_events \
             WHERE tenant_id = ? AND trace_id = ? AND name = ? \
             ORDER BY timestamp_unix_nano, span_id, event_index",
        )
        .bind(ctx.tenant_id)
        .bind(&trace_id)
        .bind(EVALUATION_EVENT)
        .fetch_all()
        .await
        .map_err(internal)?;
    let evaluations: Vec<Evaluation> = eval_rows
        .into_iter()
        .map(|r| {
            let a = ai_attributes(&r.attributes);
            let s = |k: &str| a.get(k).and_then(|v| v.as_str()).map(str::to_string);
            Evaluation {
                span_id: r.span_id,
                name: s("gen_ai.evaluation.name").unwrap_or_default(),
                score: a
                    .get("gen_ai.evaluation.score.value")
                    .and_then(|v| v.as_f64()),
                label: s("gen_ai.evaluation.score.label"),
                evaluator: s("observable.ai.evaluator"),
                explanation: s("gen_ai.evaluation.explanation"),
            }
        })
        .collect();
    let steps: Vec<InteractionStep> = rows
        .into_iter()
        .map(|r| {
            let attributes = ai_attributes(&r.attributes);
            let s = |k: &str| {
                attributes
                    .get(k)
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            };
            InteractionStep {
                operation: s("gen_ai.operation.name").unwrap_or_default(),
                model: s("gen_ai.request.model"),
                tool: s("gen_ai.tool.name"),
                span_id: r.span_id,
                parent_span_id: r.parent_span_id.filter(|p| !p.is_empty()),
                service: r.service_name,
                name: r.operation_name,
                started_at: DateTime::from_timestamp_nanos(r.start_time_unix_nano as i64),
                duration_ms: r.duration_ns as f64 / 1e6,
                status: r.status_code,
                attributes,
            }
        })
        .collect();
    let tools: Vec<&InteractionStep> = steps
        .iter()
        .filter(|s| s.operation == "execute_tool")
        .collect();
    let tool_calls = tools.len() as u64;
    let failed_tool_calls = tools.iter().filter(|s| s.status == "ERROR").count() as u64;
    let (goal_outcome, goal_evaluator) = goal_outcome(&evaluations);
    Ok(Json(InteractionResponse {
        trace_id,
        steps,
        tool_calls,
        failed_tool_calls,
        goal_outcome,
        goal_evaluator,
        evaluations,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(name: &str, label: Option<&str>) -> Evaluation {
        Evaluation {
            span_id: "s".into(),
            name: name.into(),
            score: None,
            label: label.map(str::to_string),
            evaluator: Some("judge-v2".into()),
            explanation: None,
        }
    }

    #[test]
    fn quality_without_a_window_is_unknown() {
        let w = ScoreWindow {
            mean_score: 0.9,
            evaluations: 3,
        };
        assert_eq!(
            assess_quality(Some(&w), None, 0.1).1,
            AssessmentState::Unknown
        );
        assert_eq!(
            assess_quality(None, Some(&w), 0.1).1,
            AssessmentState::Unknown
        );
    }

    #[test]
    fn spike_needs_a_baseline() {
        assert_eq!(classify_spike(10, 0, 3.0), SpikeState::NoBaseline);
        assert_eq!(classify_spike(31, 10, 3.0), SpikeState::Spike);
        assert_eq!(classify_spike(30, 10, 3.0), SpikeState::Normal);
    }

    #[test]
    fn goal_outcome_ignores_tool_calls_and_missing_evaluation_is_unknown() {
        assert_eq!(goal_outcome(&[]).0, GoalOutcome::Unknown);
        assert_eq!(
            goal_outcome(&[eval("goal_outcome", Some("failed"))]).0,
            GoalOutcome::Failed
        );
        assert_eq!(
            goal_outcome(&[eval("relevance", Some("pass"))]).0,
            GoalOutcome::Unknown
        );
    }

    #[test]
    fn drift_without_both_windows_is_unknown() {
        let (_, _, topics, state) = topic_shifts(&[("billing".into(), 0, 5)], 0.1);
        assert!(topics.is_empty());
        assert_eq!(state, AssessmentState::Unknown);
    }

    #[test]
    fn only_ai_attributes_are_returned() {
        let a =
            ai_attributes(r#"{"gen_ai.tool.name":"x","http.url":"y","observable.ai.feature":"z"}"#);
        assert!(a.contains_key("gen_ai.tool.name"));
        assert!(a.contains_key("observable.ai.feature"));
        assert!(!a.contains_key("http.url"));
    }
}
