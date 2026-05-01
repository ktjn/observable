// LLM adapter for NLQ Stage 1 (P8-S6 Step 6).
//
// Pipeline position: NLQ (user) → [this module] → NlqIr → execute_mcp_query → VisualizationFrame
//
// Design contracts (ADR-021, ADR-014):
//   - Advisory only: every response is approximate; never feed automated alerts, billing, SLA.
//   - Server-side deny gate: billing / SLA / regulatory questions are rejected before and after
//     the LLM call — prompt-only enforcement is insufficient.
//   - The LlmCaller trait is injected through AppState so tests can run without a real LLM.
//   - Service scope is enforced programmatically (not LLM-instructed) to prevent scope drift.
//   - Schema context is bounded to `schema_complete` metrics (cap 20) to stay within token budget.
use crate::mcp_query::execute_mcp_query;
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use async_openai::{
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, ResponseFormat,
    },
    Client as OpenAiClient,
};
use async_trait::async_trait;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    Json,
};
use domain::{NlqFilter, NlqFilterOp, NlqIr, NlqOperation, VisualizationFrame};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

// ── LlmCaller trait ───────────────────────────────────────────────────────────

/// Abstraction over LLM providers. Injected via AppState for testability.
///
/// The trait receives a rendered system prompt and the user's question, and returns
/// the raw JSON string produced by the model. Callers parse it into `NlqIrOrDecline`.
#[async_trait]
pub trait LlmCaller: Send + Sync {
    async fn call(&self, system_prompt: &str, question: &str) -> Result<String, LlmAdapterError>;
}

// ── OpenAI production impl ────────────────────────────────────────────────────

pub struct OpenAiLlmCaller {
    client: OpenAiClient<OpenAIConfig>,
    model: String,
}

impl OpenAiLlmCaller {
    /// Creates a caller using `LLM_API_KEY` env var. Returns None if not set.
    pub fn from_env() -> Option<Self> {
        let api_key = std::env::var("LLM_API_KEY").ok()?;
        if api_key.is_empty() {
            return None;
        }
        Some(Self::from_key(api_key, None, None))
    }

    /// Creates a caller from an explicit API key string and optional url/model overrides.
    /// If `url` or `model` are None, falls back to env vars then hardcoded defaults.
    pub fn from_key(api_key: String, url: Option<String>, model: Option<String>) -> Self {
        let model = model
            .or_else(crate::config::env_llm_model)
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let mut config = OpenAIConfig::new().with_api_key(api_key);
        let base_url = url.or_else(crate::config::env_llm_url);
        if let Some(base_url) = base_url {
            config = config.with_api_base(base_url);
        }
        Self {
            client: OpenAiClient::with_config(config),
            model,
        }
    }
    /// Fires a minimal 1-token probe completion to verify connectivity and auth.
    /// Returns `Ok(())` on success, `Err(error_message)` on failure.
    /// Costs effectively zero tokens.
    pub async fn probe(&self) -> Result<(), String> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .max_tokens(1u16)
            .messages([ChatCompletionRequestUserMessageArgs::default()
                .content("hi")
                .build()
                .map_err(|e| e.to_string())?
                .into()])
            .build()
            .map_err(|e| e.to_string())?;

        self.client
            .chat()
            .create(request)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl LlmCaller for OpenAiLlmCaller {
    async fn call(&self, system_prompt: &str, question: &str) -> Result<String, LlmAdapterError> {
        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .temperature(0.0f32)
            .response_format(ResponseFormat::JsonObject)
            .messages([
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(system_prompt)
                    .build()
                    .map_err(|e| LlmAdapterError::LlmCall(e.to_string()))?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(question)
                    .build()
                    .map_err(|e| LlmAdapterError::LlmCall(e.to_string()))?
                    .into(),
            ])
            .build()
            .map_err(|e| LlmAdapterError::LlmCall(e.to_string()))?;

        let response = self
            .client
            .chat()
            .create(request)
            .await
            .map_err(|e| LlmAdapterError::LlmCall(e.to_string()))?;

        response
            .choices
            .into_iter()
            .next()
            .and_then(|c| c.message.content)
            .ok_or_else(|| LlmAdapterError::LlmCall("empty LLM response".into()))
    }
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum LlmAdapterError {
    LlmCall(String),
    InvalidResponse(String),
    SchemaLookup(String),
    QueryExecution(crate::mcp_query::McpQueryError),
}

impl std::fmt::Display for LlmAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LlmCall(e) => write!(f, "LLM call failed: {e}"),
            Self::InvalidResponse(e) => write!(f, "invalid LLM response: {e}"),
            Self::SchemaLookup(e) => write!(f, "schema lookup failed: {e}"),
            Self::QueryExecution(e) => write!(f, "query execution failed: {e}"),
        }
    }
}

impl std::error::Error for LlmAdapterError {}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NlqQueryRequest {
    pub question: String,
    /// Optional service scope. If provided, a `service_name = <value>` filter is enforced
    /// on the generated IR regardless of what the LLM emits.
    pub service_name: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum NlqQueryResponse {
    Frame {
        frame: VisualizationFrame,
    },
    Decline {
        reason: String,
    },
    /// The LLM returned a response that could not be parsed into a valid NlqIr.
    /// Returned as HTTP 200 (not 502) so clients can display the raw output for
    /// debugging.  Treated as expected control flow, not an error.
    InvalidResponse {
        reason: String,
        raw_llm_response: String,
    },
    /// Self-description response: the assistant describes its own capabilities.
    /// No MCP call is needed — the hint is assembled server-side.
    Capabilities {
        hint: String,
    },
}

// ── LLM response discriminated union ─────────────────────────────────────────

/// Intermediate parsed form of the raw LLM JSON string.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum NlqIrOrDecline {
    Ir(NlqIr),
    Decline { reason: String },
    Capabilities,
}

// ── Server-side deny gate ─────────────────────────────────────────────────────

/// Returns a decline reason if the question appears to require BI-grade correctness or involves
/// billing/SLA/regulatory compliance that the advisory NLQ layer must not serve.
///
/// This is a belt-and-suspenders check — the LLM prompt also instructs the model to decline.
/// Prompt-only enforcement is insufficient (ADR-014).
pub fn server_side_deny_gate(question: &str) -> Option<String> {
    let lower = question.to_lowercase();

    const PROHIBITED_PATTERNS: &[(&str, &str)] = &[
        ("billing", "billing and financial reconciliation"),
        ("invoice", "invoice generation or payment processing"),
        ("sla", "SLA reporting or SLA evidence"),
        ("contractual", "contractual compliance or SLA evidence"),
        ("regulatory", "regulatory compliance reporting"),
        ("compliance report", "compliance reporting"),
        ("audit trail", "legal audit trails"),
        ("gdpr", "GDPR or regulatory data subject requests"),
        ("hipaa", "HIPAA compliance evidence"),
        ("sox", "SOX or financial audit requirements"),
    ];

    for (keyword, category) in PROHIBITED_PATTERNS {
        if lower.contains(keyword) {
            return Some(format!(
                "This question involves {category}. Observable NLQ delivers approximate operational \
                 insights only and is not suitable for billing, SLA evidence, contractual compliance, \
                 or regulatory reporting. Please use a certified data pipeline or BI tool for this purpose."
            ));
        }
    }
    None
}

// ── Schema context ────────────────────────────────────────────────────────────

/// Fetches up to `limit` schema-complete metrics for the tenant, ordered by annotation richness.
///
/// Delegates to `mcp_tools::list_signal_fields` — the canonical home for schema lookups —
/// and filters for `schema_complete = true` (metric_type + timestamp_column both present),
/// which is the minimum annotation required for correct MCP SQL generation.
async fn fetch_schema_context(
    db: &PgPool,
    tenant_id: Uuid,
    limit: usize,
) -> Result<Vec<crate::mcp_tools::SignalField>, LlmAdapterError> {
    let mut fields = crate::mcp_tools::list_signal_fields(db, tenant_id, "metrics")
        .await
        .map_err(|e| LlmAdapterError::SchemaLookup(e.to_string()))?;

    // Retain only schema-complete entries and order by annotation richness so the most
    // informative metrics appear first in the LLM prompt (token budget awareness).
    fields.retain(|f| f.schema_complete);
    fields.sort_by_key(|f| {
        // Negate to sort richest first.
        let score = f.business_description.is_some() as i32 + f.display_name.is_some() as i32;
        -score
    });
    fields.truncate(limit);
    Ok(fields)
}

// ── System prompt builder ─────────────────────────────────────────────────────

pub(crate) fn build_system_prompt(
    metrics: &[crate::mcp_tools::SignalField],
    service_scope: Option<&str>,
) -> String {
    let mut prompt = String::from(
        r#"You are an observability query assistant. You translate natural language questions
about operational metrics into a structured NLQ IR (intermediate representation) that an
observability platform uses to query time-series data.

## Output format

Respond with JSON only. Use EXACTLY one of these three schemas:

If you can answer the question:
{"type": "ir", "ir": <NlqIr object>}

If the question is outside scope (billing, SLA evidence, regulatory compliance, financial
reconciliation) or you cannot produce a valid IR:
{"type": "decline", "reason": "<brief explanation>"}

If the user asks about your own capabilities ("what can you query", "what operations",
"describe yourself", "what can I ask", "what metrics are available", "how do I use you"):
{"type": "capabilities"}
Do not attempt to generate an IR for these meta-questions.

## NlqIr schema

{
  "operation": "timeseries" | "rate" | "irate" | "increase" | "histogram" | "topk" | "table" | "distribution" | "catalog",
  "signals": ["metrics"],
  "metric": "<metric_name_from_schema_below>",
  "window": "5m" | null,
  "filters": [{"field": "<field>", "op": "=" | "!=" | ">" | ">=" | "<" | "<=" | "=~" | "!~", "value": "<val>"}],
  "group_by": ["<field>"],
  "resolution": "1m" | "5m" | "1h" | null,
  "time_range": {"from": "now-1h", "to": "now"},
  "visualization_hint": "timeseries" | "histogram" | "heatmap" | "table" | "topk" | "flamegraph" | "distribution" | null,
  "percentiles": ["p99"] | ["p75","p95","p99","average","median"] | null,
  "catalog_field": "service_name" | "environment" | "metric_name" | "<any_label_key>" | null
}

IMPORTANT — the `signals` field is a signal CATEGORY, not the metric name.
It MUST be one of: "metrics", "traces", or "logs". For metric questions always use ["metrics"].
The metric name goes in the `metric` field, never in `signals`.

## Operation guide

- timeseries: gauge average over time buckets (use for most questions about "over time")
- rate: per-second rate of a counter (resets-aware)
- irate: instantaneous rate from two most recent samples
- increase: total increase of a counter over the window
- histogram: bucket distribution (only for histogram metrics)
- topk: top-N series by average value
- table: raw point scan, most recent 1000 rows
- distribution: compute only the stats the user asked for
- **catalog**: Enumerate distinct observable entities (metadata). Use when the user asks "list X", "what X exist?", "how many X?", "which X does Y have?". Set `catalog_field` to the dimension name: "service_name", "environment", "metric_name", or any label key like "pod", "region", "namespace". Does NOT require a `metric` field.
  Example: "list all services" → {"operation":"catalog","signals":["metrics"],"catalog_field":"service_name","filters":[],"time_range":{"from":"now-24h","to":"now"}}
  Example: "what pods does checkout use?" → {"operation":"catalog","signals":["metrics"],"catalog_field":"pod","filters":[{"field":"service_name","op":"=","value":"checkout"}],"time_range":{"from":"now-24h","to":"now"}}
  Example: "what metrics does payments emit?" → {"operation":"catalog","signals":["metrics"],"catalog_field":"metric_name","filters":[{"field":"service_name","op":"=","value":"payments"}],"time_range":{"from":"now-24h","to":"now"}}

## `percentiles` field (for distribution operation only)

Set to EXACTLY the stats the user asked for — no more, no less:
- `"p{N}"` for any N from 1–999 (e.g. `"p50"`, `"p75"`, `"p95"`, `"p99"`, `"p999"`)
- `"median"` (same as p50)
- `"average"` or `"mean"` (arithmetic mean)
- `"min"`, `"max"`

Examples:
- User asked **"p99 latency"** → `"percentiles": ["p99"]`
- User asked **"p75, p95, p99"** → `"percentiles": ["p75", "p95", "p99"]`
- User asked **"p99, average, and median"** → `"percentiles": ["p99", "average", "median"]`
- User asked **"distribution"** or **"all percentiles"** → omit `percentiles` entirely (null)

NEVER include percentiles the user did not ask for.

## Advisory boundary — MANDATORY

You MUST emit {"type": "decline", ...} for questions involving:
- Billing, invoicing, or financial reconciliation
- SLA evidence, contractual compliance, or service level objectives used as contracts
- Regulatory compliance (GDPR, HIPAA, SOX, audit trails)
- Any use case requiring BI-grade correctness guarantees

## Available metrics (schema_complete only)

"#,
    );

    if metrics.is_empty() {
        prompt.push_str("(no annotated metrics available for this tenant)\n");
    } else {
        for m in metrics {
            prompt.push_str(&format!("- **{}**", m.field_name));
            if let Some(dn) = &m.display_name {
                prompt.push_str(&format!(" ({dn})"));
            }
            if let Some(mt) = &m.metric_type {
                prompt.push_str(&format!(" [type: {mt}]"));
            }
            if let Some(u) = &m.unit {
                prompt.push_str(&format!(" [unit: {u}]"));
            }
            if let Some(desc) = &m.business_description {
                prompt.push_str(&format!(": {desc}"));
            }
            if let Some(rule) = &m.interpretation_rule {
                prompt.push_str(&format!(" Interpretation: {rule}"));
            }
            if let Some(rate) = m.effective_sample_rate {
                if rate < 1.0 {
                    prompt.push_str(&format!(" [sampled at {:.0}%]", rate * 100.0));
                }
            }
            prompt.push('\n');
        }
    }

    if let Some(svc) = service_scope {
        prompt.push_str(&format!(
            "\n## Service scope\n\nThis query is scoped to service `{svc}`. \
             You do not need to add a service_name filter — it is enforced automatically.\n"
        ));
    }

    prompt
}

// ── Capabilities hint ─────────────────────────────────────────────────────────

/// Builds a static capabilities description. Assembled server-side — no LLM or DB call needed.
fn build_capabilities_hint() -> String {
    r#"Observable NLQ supports the following operations:

**Operations:**
- timeseries  — gauge average over time buckets
- rate        — per-second rate of a counter (reset-aware)
- irate       — instantaneous rate from two most recent samples
- increase    — total counter increase over a window
- histogram   — bucket distribution (for histogram metrics)
- topk        — top-N series by average value
- table       — raw point scan (most recent 1000 rows)
- distribution — compute specific percentiles (p50, p75, p95, p99, median, average, min, max)
- catalog     — list distinct values of a dimension (service_name, environment, metric_name, or any label)

**Filters:** =  !=  >  >=  <  <=  =~  !~
**Time range syntax:** relative (now-1h, now-30m, now-7d) or ISO-8601 timestamps

**Example questions:**
- "p99 latency for checkout over the last hour"
- "request rate for payments, grouped by pod"
- "list all services"
- "what metrics does checkout emit?"
- "top 10 services by CPU usage"
- "error rate for all services in production over last 24 hours"

Advisory only: results are approximate and must not be used for billing, SLA enforcement, or regulatory compliance."#.into()
}

// ── Parse LLM response ────────────────────────────────────────────────────────

/// Normalises the `signals` array in the raw LLM JSON before serde deserialization.
///
/// Small LLMs frequently confuse the `signals` field (a signal category: "metrics",
/// "traces", or "logs") with the metric name they found in the schema context.  This
/// function replaces any unrecognised signal value with "metrics" and defaults an
/// empty or missing array to `["metrics"]`.  This makes the pipeline robust to the
/// most common LLM schema-compliance failure without changing the domain type.
fn normalize_nlq_signals(ir_val: &mut serde_json::Value) {
    const VALID: &[&str] = &["metrics", "traces", "logs"];

    let signals = ir_val.get_mut("signals");
    match signals {
        Some(serde_json::Value::Array(arr)) if !arr.is_empty() => {
            for s in arr.iter_mut() {
                if s.as_str().map(|v| !VALID.contains(&v)).unwrap_or(true) {
                    *s = serde_json::Value::String("metrics".into());
                }
            }
        }
        // Missing or empty array → default to ["metrics"]
        _ => {
            ir_val["signals"] = serde_json::json!(["metrics"]);
        }
    }
}

pub(crate) fn parse_llm_response(json: &str) -> Result<NlqIrOrDecline, LlmAdapterError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| LlmAdapterError::InvalidResponse(format!("JSON parse failed: {e}")))?;

    match v.get("type").and_then(|t| t.as_str()) {
        Some("ir") => {
            let mut ir_val = v
                .get("ir")
                .ok_or_else(|| LlmAdapterError::InvalidResponse("missing 'ir' field".into()))?
                .clone();
            normalize_nlq_signals(&mut ir_val);
            let ir: NlqIr = serde_json::from_value(ir_val).map_err(|e| {
                LlmAdapterError::InvalidResponse(format!("NlqIr deserialize failed: {e}"))
            })?;
            Ok(NlqIrOrDecline::Ir(ir))
        }
        Some("decline") => {
            let reason = v
                .get("reason")
                .and_then(|r| r.as_str())
                .unwrap_or("Query declined by assistant")
                .to_string();
            Ok(NlqIrOrDecline::Decline { reason })
        }
        Some("capabilities") => Ok(NlqIrOrDecline::Capabilities),
        other => Err(LlmAdapterError::InvalidResponse(format!(
            "unexpected type field: {other:?}"
        ))),
    }
}

// ── Service scope enforcement ─────────────────────────────────────────────────

/// Injects a `service_name = <value>` filter if `service_name` is specified and not already
/// present in the IR. This is enforced server-side, not LLM-instructed.
fn enforce_service_scope(ir: &mut NlqIr, service_name: &str) {
    let already_scoped = ir.filters.iter().any(|f| f.field == "service_name");
    if !already_scoped {
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: service_name.into(),
        });
    }
}

// ── Core orchestration ────────────────────────────────────────────────────────

/// End-to-end NLQ pipeline: question → LlmCaller → NlqIr → execute_mcp_query → NlqQueryResponse.
///
/// Advisory-only: every VisualizationFrame carries provenance (source_sql, approximation_statement).
/// Callers must not use results for billing, SLA enforcement, or regulatory compliance.
pub async fn run_nlq_pipeline(
    db: &PgPool,
    ch: &clickhouse::Client,
    llm: &dyn LlmCaller,
    tenant_id: Uuid,
    req: &NlqQueryRequest,
) -> Result<NlqQueryResponse, LlmAdapterError> {
    let pipeline_start = std::time::Instant::now();
    let question_preview = req.question.chars().take(256).collect::<String>();

    tracing::info!(
        tenant_id = %tenant_id,
        question = %question_preview,
        service_scope = ?req.service_name,
        "NLQ pipeline started"
    );

    // 1. Server-side deny gate (belt and suspenders — LLM prompt also instructs decline)
    if let Some(reason) = server_side_deny_gate(&req.question) {
        tracing::info!(
            tenant_id = %tenant_id,
            question = %question_preview,
            reason = %reason,
            "NLQ declined by server-side deny gate"
        );
        return Ok(NlqQueryResponse::Decline { reason });
    }

    // 2. Fetch bounded schema context (up to 20 schema-complete metrics)
    let metrics = fetch_schema_context(db, tenant_id, 20).await?;

    // 3. Build system prompt
    let system_prompt = build_system_prompt(&metrics, req.service_name.as_deref());

    tracing::debug!(
        tenant_id = %tenant_id,
        prompt_metric_count = metrics.len(),
        "NLQ calling LLM"
    );

    // 4. Call LLM
    let llm_start = std::time::Instant::now();
    let raw_response = llm.call(&system_prompt, &req.question).await?;
    let llm_elapsed_ms = llm_start.elapsed().as_millis();

    tracing::debug!(
        tenant_id = %tenant_id,
        llm_elapsed_ms,
        raw_response_len = raw_response.len(),
        "NLQ LLM call complete"
    );

    // 5. Parse LLM response
    let parsed = match parse_llm_response(&raw_response) {
        Ok(p) => {
            tracing::debug!(
                tenant_id = %tenant_id,
                parsed_type = match &p {
                    NlqIrOrDecline::Ir(_) => "ir",
                    NlqIrOrDecline::Decline { .. } => "decline",
                    NlqIrOrDecline::Capabilities => "capabilities",
                },
                "NLQ LLM response parsed"
            );
            p
        }
        Err(LlmAdapterError::InvalidResponse(ref reason)) => {
            let truncated = raw_response.chars().take(512).collect::<String>();
            tracing::warn!(
                tenant_id = %tenant_id,
                question = %question_preview,
                error = %reason,
                raw_response = %truncated,
                llm_elapsed_ms,
                "NLQ LLM returned unparseable response"
            );
            return Ok(NlqQueryResponse::InvalidResponse {
                reason: reason.clone(),
                raw_llm_response: raw_response,
            });
        }
        Err(e) => return Err(e),
    };

    // 6. Handle decline or capabilities from LLM
    let mut ir = match parsed {
        NlqIrOrDecline::Decline { reason } => {
            tracing::info!(
                tenant_id = %tenant_id,
                question = %question_preview,
                reason = %reason,
                source = "llm",
                "NLQ declined by LLM"
            );
            return Ok(NlqQueryResponse::Decline { reason });
        }
        NlqIrOrDecline::Capabilities => {
            tracing::info!(
                tenant_id = %tenant_id,
                question = %question_preview,
                "NLQ capabilities short-circuit"
            );
            return Ok(NlqQueryResponse::Capabilities {
                hint: build_capabilities_hint(),
            });
        }
        NlqIrOrDecline::Ir(ir) => ir,
    };

    // 7. Enforce service scope if provided (server-side, not LLM-dependent)
    if let Some(svc) = &req.service_name {
        enforce_service_scope(&mut ir, svc);
    }

    // 8. Validate: metric is required for non-catalog operations
    if ir.metric.is_none() && ir.operation != NlqOperation::Catalog {
        tracing::info!(
            tenant_id = %tenant_id,
            question = %question_preview,
            source = "no_metric",
            "NLQ declined — no metric identified"
        );
        return Ok(NlqQueryResponse::Decline {
            reason: "Could not identify a metric for this question. \
                     Please rephrase or specify a metric name."
                .into(),
        });
    }

    tracing::debug!(
        tenant_id = %tenant_id,
        operation = ?ir.operation,
        metric = ?ir.metric,
        "NLQ executing MCP query"
    );

    // 9. Execute MCP query
    let mcp_start = std::time::Instant::now();
    let frame = execute_mcp_query(db, ch, tenant_id, &ir)
        .await
        .map_err(LlmAdapterError::QueryExecution)?;
    let mcp_elapsed_ms = mcp_start.elapsed().as_millis();
    let pipeline_elapsed_ms = pipeline_start.elapsed().as_millis();

    tracing::debug!(
        tenant_id = %tenant_id,
        mcp_elapsed_ms,
        row_count = frame.data.len(),
        "NLQ MCP query complete"
    );

    tracing::info!(
        tenant_id = %tenant_id,
        pipeline_elapsed_ms,
        result = "frame",
        "NLQ pipeline complete"
    );

    Ok(NlqQueryResponse::Frame { frame })
}

// ── HTTP handler ──────────────────────────────────────────────────────────────

/// POST /v1/nlq
///
/// Accepts a natural language question, calls the LLM adapter, and returns a discriminated
/// `NlqQueryResponse` (frame or decline).
///
/// Returns 503 if no LLM configuration exists at all (neither key nor endpoint URL in env or DB).
pub async fn handle_nlq_query(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<NlqQueryRequest>,
) -> Result<Json<NlqQueryResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Resolve the LLM caller: prefer pre-built caller in AppState (from env var at startup),
    // fall back to constructing one from the DB-stored config at call time.
    //
    // No-auth providers (Ollama, local vLLM) may have only a URL + model configured and no API
    // key. An absent key is treated as an empty string so those providers work out of the box,
    // mirroring what `test_llm_connection` does.  503 is only returned when neither key nor URL
    // is configured in env or DB — i.e., the user has not done any LLM setup at all.
    let db_caller: Option<OpenAiLlmCaller>;
    let llm: &dyn LlmCaller = if let Some(ref arc) = state.llm {
        arc.as_ref()
    } else {
        let api_key = crate::config::fetch_db_key(&state.db).await.map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database error resolving LLM config"})),
            )
        })?;
        let url = crate::config::fetch_db_value(&state.db, "llm_url")
            .await
            .ok()
            .flatten()
            .filter(|v| !v.is_empty());
        let model = crate::config::fetch_db_value(&state.db, "llm_model")
            .await
            .ok()
            .flatten()
            .filter(|v| !v.is_empty());

        if api_key.is_none() && url.is_none() {
            return Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "LLM adapter not configured — set an API key or endpoint URL on the Setup page"
                })),
            ));
        }

        db_caller = Some(OpenAiLlmCaller::from_key(
            api_key.unwrap_or_default(),
            url,
            model,
        ));
        db_caller.as_ref().unwrap()
    };

    if req.question.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "question is required"})),
        ));
    }

    match run_nlq_pipeline(&state.db, &state.ch, llm, ctx.tenant_id, &req).await {
        Ok(response) => Ok(Json(response)),
        Err(LlmAdapterError::LlmCall(e)) => {
            tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "LLM call failed");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("LLM call failed: {e}")})),
            ))
        }
        Err(e) => {
            tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "NLQ pipeline failed");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "NLQ pipeline failed"})),
            ))
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{NlqOperation, NlqSignal, NlqTimeRange};

    // ── Mock LlmCaller ────────────────────────────────────────────────────────

    struct MockLlmCaller {
        response: String,
    }

    impl MockLlmCaller {
        fn with_ir(ir: &NlqIr) -> Self {
            let json = serde_json::to_string(&serde_json::json!({
                "type": "ir",
                "ir": ir
            }))
            .unwrap();
            Self { response: json }
        }

        fn with_decline(reason: &str) -> Self {
            Self {
                response: serde_json::to_string(&serde_json::json!({
                    "type": "decline",
                    "reason": reason
                }))
                .unwrap(),
            }
        }

        fn with_raw(raw: &str) -> Self {
            Self {
                response: raw.to_string(),
            }
        }
    }

    #[async_trait]
    impl LlmCaller for MockLlmCaller {
        async fn call(&self, _system: &str, _question: &str) -> Result<String, LlmAdapterError> {
            Ok(self.response.clone())
        }
    }

    fn sample_ir(metric: &str) -> NlqIr {
        NlqIr {
            operation: NlqOperation::Timeseries,
            signals: vec![NlqSignal::Metrics],
            metric: Some(metric.into()),
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: Some("1m".into()),
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
        }
    }

    // ── server_side_deny_gate ─────────────────────────────────────────────────

    #[test]
    fn deny_gate_blocks_billing_question() {
        let result = server_side_deny_gate("What is our billing total this month?");
        assert!(result.is_some(), "billing question must be denied");
    }

    #[test]
    fn deny_gate_blocks_sla_question() {
        let result = server_side_deny_gate("Did we meet our SLA for the checkout service?");
        assert!(result.is_some(), "SLA question must be denied");
    }

    #[test]
    fn deny_gate_blocks_regulatory_question() {
        let result = server_side_deny_gate("Generate GDPR compliance report");
        assert!(result.is_some(), "regulatory question must be denied");
    }

    #[test]
    fn deny_gate_allows_operational_question() {
        let result = server_side_deny_gate(
            "Show me p99 latency for the checkout service over the last hour",
        );
        assert!(
            result.is_none(),
            "operational question must pass the deny gate"
        );
    }

    #[test]
    fn deny_gate_allows_error_rate_question() {
        let result = server_side_deny_gate("What is the error rate for payment service?");
        assert!(result.is_none(), "error rate question must pass deny gate");
    }

    // ── parse_llm_response ────────────────────────────────────────────────────

    #[test]
    fn parse_valid_ir_response() {
        let ir = sample_ir("latency_ms");
        let json = serde_json::to_string(&serde_json::json!({"type": "ir", "ir": ir})).unwrap();
        match parse_llm_response(&json).unwrap() {
            NlqIrOrDecline::Ir(parsed_ir) => {
                assert_eq!(parsed_ir.metric.as_deref(), Some("latency_ms"));
            }
            NlqIrOrDecline::Decline { .. } => panic!("expected Ir, got Decline"),
            NlqIrOrDecline::Capabilities => panic!("expected Ir, got Capabilities"),
        }
    }

    #[test]
    fn parse_valid_decline_response() {
        let json = r#"{"type": "decline", "reason": "This involves billing data"}"#;
        match parse_llm_response(json).unwrap() {
            NlqIrOrDecline::Decline { reason } => {
                assert!(reason.contains("billing"));
            }
            NlqIrOrDecline::Ir(_) => panic!("expected Decline, got Ir"),
            NlqIrOrDecline::Capabilities => panic!("expected Decline, got Capabilities"),
        }
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let result = parse_llm_response("not json at all");
        assert!(
            matches!(result, Err(LlmAdapterError::InvalidResponse(_))),
            "invalid JSON must yield InvalidResponse"
        );
    }

    #[test]
    fn parse_missing_type_field_returns_error() {
        let json = r#"{"ir": {"operation": "timeseries"}}"#;
        let result = parse_llm_response(json);
        assert!(
            matches!(result, Err(LlmAdapterError::InvalidResponse(_))),
            "missing type field must yield InvalidResponse"
        );
    }

    #[test]
    fn parse_unknown_type_field_returns_error() {
        let json = r#"{"type": "unknown", "data": {}}"#;
        let result = parse_llm_response(json);
        assert!(
            matches!(result, Err(LlmAdapterError::InvalidResponse(_))),
            "unknown type value must yield InvalidResponse"
        );
    }

    #[test]
    fn parse_ir_with_bad_shape_returns_error() {
        // operation is required in NlqIr
        let json = r#"{"type": "ir", "ir": {"metric": "latency_ms"}}"#;
        let result = parse_llm_response(json);
        assert!(
            matches!(result, Err(LlmAdapterError::InvalidResponse(_))),
            "bad NlqIr shape must yield InvalidResponse"
        );
    }

    // ── normalize_nlq_signals ─────────────────────────────────────────────────

    fn make_ir_json_with_signals(signals: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "type": "ir",
            "ir": {
                "operation": "timeseries",
                "signals": signals,
                "metric": "latency_ms",
                "window": null,
                "filters": [],
                "group_by": [],
                "resolution": "1m",
                "time_range": {"from": "now-1h", "to": "now"},
                "visualization_hint": null
            }
        })
    }

    #[test]
    fn parse_signal_with_metric_name_normalized_to_metrics() {
        // phi3 and other small LLMs put the metric name in signals; must be normalised.
        let json = make_ir_json_with_signals(serde_json::json!(["request_duration_ms"]));
        match parse_llm_response(&json.to_string()).unwrap() {
            NlqIrOrDecline::Ir(ir) => {
                assert_eq!(ir.signals, vec![NlqSignal::Metrics]);
            }
            NlqIrOrDecline::Decline { .. } => panic!("expected Ir, got Decline"),
            NlqIrOrDecline::Capabilities => panic!("expected Ir, got Capabilities"),
        }
    }

    #[test]
    fn parse_signal_empty_array_defaults_to_metrics() {
        let json = make_ir_json_with_signals(serde_json::json!([]));
        match parse_llm_response(&json.to_string()).unwrap() {
            NlqIrOrDecline::Ir(ir) => {
                assert_eq!(ir.signals, vec![NlqSignal::Metrics]);
            }
            NlqIrOrDecline::Decline { .. } => panic!("expected Ir, got Decline"),
            NlqIrOrDecline::Capabilities => panic!("expected Ir, got Capabilities"),
        }
    }

    #[test]
    fn parse_signal_missing_field_defaults_to_metrics() {
        let json = serde_json::json!({
            "type": "ir",
            "ir": {
                "operation": "timeseries",
                // no "signals" field
                "metric": "latency_ms",
                "window": null,
                "filters": [],
                "group_by": [],
                "resolution": "1m",
                "time_range": {"from": "now-1h", "to": "now"},
                "visualization_hint": null
            }
        });
        match parse_llm_response(&json.to_string()).unwrap() {
            NlqIrOrDecline::Ir(ir) => {
                assert_eq!(ir.signals, vec![NlqSignal::Metrics]);
            }
            NlqIrOrDecline::Decline { .. } => panic!("expected Ir, got Decline"),
            NlqIrOrDecline::Capabilities => panic!("expected Ir, got Capabilities"),
        }
    }

    #[test]
    fn parse_mixed_signals_unknown_values_normalized() {
        // Valid + invalid mixture: unknown values replaced, valid ones kept.
        let json =
            make_ir_json_with_signals(serde_json::json!(["metrics", "bad_metric_name", "logs"]));
        match parse_llm_response(&json.to_string()).unwrap() {
            NlqIrOrDecline::Ir(ir) => {
                assert!(ir.signals.contains(&NlqSignal::Metrics));
                assert!(ir.signals.contains(&NlqSignal::Logs));
                // The unknown value should have been replaced with "metrics"
                assert_eq!(ir.signals.len(), 3);
            }
            NlqIrOrDecline::Decline { .. } => panic!("expected Ir, got Decline"),
            NlqIrOrDecline::Capabilities => panic!("expected Ir, got Capabilities"),
        }
    }

    #[test]
    fn service_scope_filter_injected_when_absent() {
        let mut ir = sample_ir("latency_ms");
        enforce_service_scope(&mut ir, "checkout");
        assert!(
            ir.filters
                .iter()
                .any(|f| f.field == "service_name" && f.value == "checkout"),
            "service_name filter must be injected"
        );
    }

    #[test]
    fn service_scope_not_duplicated_when_already_present() {
        let mut ir = sample_ir("latency_ms");
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "checkout".into(),
        });
        let pre_count = ir.filters.len();
        enforce_service_scope(&mut ir, "checkout");
        assert_eq!(
            ir.filters.len(),
            pre_count,
            "existing service_name filter must not be duplicated"
        );
    }

    // ── build_system_prompt ───────────────────────────────────────────────────

    #[test]
    fn system_prompt_includes_metric_names() {
        let metrics = vec![crate::mcp_tools::SignalField {
            field_name: "latency_ms".into(),
            field_type: "float64".into(),
            otel_spec_version: None,
            display_name: Some("Request Latency".into()),
            business_description: Some("End-to-end request latency".into()),
            interpretation_rule: None,
            effective_sample_rate: None,
            metric_type: Some("gauge".into()),
            timestamp_column: Some("timestamp_unix_nano".into()),
            unit: Some("ms".into()),
            recommended_downsampling: None,
            schema_complete: true,
        }];
        let prompt = build_system_prompt(&metrics, None);
        assert!(
            prompt.contains("latency_ms"),
            "metric name must appear in prompt"
        );
        assert!(
            prompt.contains("Request Latency"),
            "display name must appear in prompt"
        );
    }

    #[test]
    fn system_prompt_includes_service_scope() {
        let prompt = build_system_prompt(&[], Some("checkout"));
        assert!(
            prompt.contains("checkout"),
            "service scope must appear in prompt"
        );
    }

    #[test]
    fn system_prompt_includes_advisory_boundary() {
        let prompt = build_system_prompt(&[], None);
        assert!(
            prompt.contains("billing"),
            "advisory boundary must appear in prompt"
        );
        assert!(
            prompt.contains("decline"),
            "decline instruction must appear in prompt"
        );
    }

    // ── MockLlmCaller integration ─────────────────────────────────────────────

    #[tokio::test]
    async fn mock_caller_returns_canned_response() {
        let ir = sample_ir("latency_ms");
        let caller = MockLlmCaller::with_ir(&ir);
        let result = caller.call("system", "question").await.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["type"], "ir");
    }

    #[tokio::test]
    async fn mock_caller_decline_returns_decline() {
        let caller = MockLlmCaller::with_decline("test reason");
        let raw = caller.call("system", "question").await.unwrap();
        match parse_llm_response(&raw).unwrap() {
            NlqIrOrDecline::Decline { reason } => assert_eq!(reason, "test reason"),
            _ => panic!("expected decline"),
        }
    }

    #[tokio::test]
    async fn mock_caller_with_raw_invalid_json_yields_error() {
        let caller = MockLlmCaller::with_raw("not json");
        let raw = caller.call("system", "question").await.unwrap();
        assert!(parse_llm_response(&raw).is_err());
    }

    // ── capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn parse_capabilities_response_returns_capabilities_variant() {
        let json = r#"{"type": "capabilities"}"#;
        match parse_llm_response(json).unwrap() {
            NlqIrOrDecline::Capabilities => {}
            other => panic!("expected Capabilities, got {other:?}"),
        }
    }

    #[test]
    fn build_capabilities_hint_contains_operations() {
        let hint = build_capabilities_hint();
        assert!(hint.contains("timeseries"), "hint must list timeseries");
        assert!(hint.contains("catalog"), "hint must list catalog");
        assert!(hint.contains("distribution"), "hint must list distribution");
        assert!(hint.contains("billing"), "hint must include advisory note");
    }

    #[test]
    fn system_prompt_includes_capabilities_instruction() {
        let prompt = build_system_prompt(&[], None);
        assert!(
            prompt.contains("capabilities"),
            "system prompt must reference capabilities type"
        );
    }
}
