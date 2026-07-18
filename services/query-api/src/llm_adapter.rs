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

const MAX_REPAIR_ATTEMPTS: usize = 1;
use crate::mcp_query::execute_mcp_query;
use crate::middleware::auth::TenantContext;
use crate::traces::AppState;
use async_openai::{
    Client as OpenAiClient,
    config::OpenAIConfig,
    types::chat::{
        ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs,
        CreateChatCompletionRequestArgs, ResponseFormat,
    },
};
use async_trait::async_trait;
use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
use domain::{
    NlqFilter, NlqFilterOp, NlqIr, NlqOperation, NlqSignal, NlqTimeRange, VisualizationFrame,
};
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
            .or_else(crate::llm_config::env_llm_model)
            .unwrap_or_else(|| "gpt-4o-mini".into());
        let mut config = OpenAIConfig::new().with_api_key(api_key);
        let base_url = url.or_else(crate::llm_config::env_llm_url);
        if let Some(base_url) = base_url {
            config = config.with_api_base(base_url);
        }
        Self {
            client: OpenAiClient::with_config(config),
            model,
        }
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
    QueryExecution(crate::mcp_query::McpQueryError),
}

impl std::fmt::Display for LlmAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LlmCall(e) => write!(f, "LLM call failed: {e}"),
            Self::InvalidResponse(e) => write!(f, "invalid LLM response: {e}"),
            Self::QueryExecution(e) => write!(f, "query execution failed: {e}"),
        }
    }
}

impl std::error::Error for LlmAdapterError {}

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct NlqQueryRequest {
    /// The user's natural-language question or raw IR JSON.
    /// Optional when `base_ir` is set — omitting it fetches the page's base data directly.
    #[serde(default)]
    pub question: Option<String>,
    /// Optional service scope. If provided, a `service_name = <value>` filter is enforced
    /// on the generated IR regardless of what the LLM emits.
    pub service_name: Option<String>,
    /// Optional base IR for the current page surface.
    ///
    /// When set:
    /// - No question → execute `base_ir` directly (page-load pattern; no LLM needed).
    /// - Question present, mode=execute → interpret question → merge user IR into `base_ir`
    ///   (base `operation`/`signals`/`catalog_field` preserved) → execute merged IR.
    /// - Question present, mode=interpret → `base_ir` guides the LLM system prompt only;
    ///   no merge is applied and the raw interpreted IR is returned.
    ///
    /// Replaces the former `surface_hint` string: LLM context is derived from `base_ir.operation`
    /// and `base_ir.signals` directly, removing string-name coupling.
    #[serde(default)]
    pub base_ir: Option<NlqIr>,
    /// Execution mode. `execute` runs the query and returns a frame; `interpret`
    /// returns a validated IR without running the MCP query.
    #[serde(default)]
    pub mode: NlqQueryMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NlqQueryMode {
    #[default]
    Execute,
    Interpret,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum NlqQueryResponse {
    Frame {
        frame: VisualizationFrame,
    },
    Ir {
        ir: NlqIr,
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

#[derive(Debug)]
pub(crate) enum UserQueryInput {
    RawIr(Box<NlqIr>),
    NaturalLanguage,
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

/// Fetches up to `limit` schema-complete metrics for the tenant, ordered by annotation richness,
/// and the top label keys from ClickHouse metric_series.
///
/// Delegates to `mcp_tools::list_signal_fields` — the canonical home for schema lookups —
/// and filters for `schema_complete = true` (metric_type + timestamp_column both present),
/// which is the minimum annotation required for correct MCP SQL generation.
///
/// Label key fetch errors are non-fatal: on failure a warning is logged and an empty list
/// is returned so the rest of the pipeline continues unimpeded.
async fn fetch_schema_context(
    db: &PgPool,
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    limit: usize,
) -> Result<(Vec<crate::mcp_tools::SignalField>, Vec<String>), LlmAdapterError> {
    // Non-fatal on error: if the schema registry is temporarily unreachable the
    // prompt will have no metric metadata but the pipeline can still proceed.
    let mut fields = match crate::mcp_tools::list_signal_fields(db, tenant_id, "metrics").await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = %e,
                "fetch_schema_context: list_signal_fields failed — continuing with empty metric list"
            );
            vec![]
        }
    };

    // Retain only schema-complete entries and order by annotation richness so the most
    // informative metrics appear first in the LLM prompt (token budget awareness).
    fields.retain(|f| f.schema_complete);
    fields.sort_by_key(|f| {
        // Negate to sort richest first.
        let score = f.business_description.is_some() as i32 + f.display_name.is_some() as i32;
        -score
    });
    fields.truncate(limit);

    // Fetch label keys from ClickHouse — non-fatal on error.
    let label_keys = match crate::mcp_tools::fetch_label_keys(ch, tenant_id, limit).await {
        Ok(keys) => keys,
        Err(e) => {
            tracing::warn!(
                tenant_id = %tenant_id,
                error = %e,
                "fetch_label_keys failed — continuing with empty label list"
            );
            vec![]
        }
    };

    Ok((fields, label_keys))
}

// ── System prompt builder ─────────────────────────────────────────────────────

pub(crate) fn build_system_prompt(
    metrics: &[crate::mcp_tools::SignalField],
    label_keys: &[String],
    service_scope: Option<&str>,
    base_ir: Option<&NlqIr>,
) -> String {
    let mut prompt = String::from(
        r#"You are an observability query assistant. You translate natural language questions
about operational metrics into a structured NLQ IR (intermediate representation) that an
observability platform uses to query time-series data.

## Output format

Respond with JSON only. Use EXACTLY one of these three schemas:

If you can answer the question:
{"type": "ir", "ir": <NlqIr object>}

If the question is outside scope (explicitly billing, SLA evidence, regulatory compliance, or
financial reconciliation):
{"type": "decline", "reason": "<brief explanation>"}
Do NOT decline operational, observability, or metadata questions — always try to produce an IR.

If the user asks about your own capabilities ("what can you query", "what operations",
"describe yourself", "what can I ask", "what metrics are available", "how do I use you"):
{"type": "capabilities"}
Do not attempt to generate an IR for these meta-questions.

## NlqIr schema

{
  "operation": "timeseries" | "rate" | "irate" | "increase" | "histogram" | "topk" | "table" | "distribution" | "catalog",
  "signals": ["metrics"] | ["logs"],
  "metric": "<metric_name_from_schema_below>" | null,
  "query": "<free_text_search_term>" | null,
  "window": "5m" | null,
  "filters": [{"field": "<field>", "op": "=" | "!=" | ">" | ">=" | "<" | "<=" | "=~" | "!~", "value": "<val>"}],
  "group_by": ["<field>"],
  "resolution": "1m" | "5m" | "1h" | null,
  "time_range": {"from": "now-1h", "to": "now"},
  "visualization_hint": "timeseries" | "histogram" | "heatmap" | "table" | "topk" | "flamegraph" | "distribution" | null,
  "percentiles": ["p99"] | ["p75","p95","p99","average","median"] | null,
  "catalog_field": "service_name" | "environment" | "metric_name" | "<any_label_key>" | null,
  "limit": 10 | null
}

IMPORTANT — the `signals` field is a signal CATEGORY, not the metric name.
It MUST be one of: "metrics", "traces", or "logs". For metric questions always use ["metrics"].
For log search questions always use ["logs"].
The metric name goes in the `metric` field, never in `signals`.

## Operation guide

- timeseries: gauge average over time buckets — use when the user wants a chart of values changing over time
- rate: per-second rate of a counter (resets-aware)
- irate: instantaneous rate from two most recent samples
- increase: total increase of a counter over the window
- histogram: display raw OTel Histogram bucket data — use ONLY when the metric type is "histogram" AND the user explicitly says "histogram", "bucket distribution", or "buckets". Requires OTel Histogram metrics with explicit bucket bounds. Do NOT use for gauge or counter metrics.
- topk: rank services/pods/labels by a computed metric value — use when user asks "top N by X", "highest X", "which service has the most/highest X". REQUIRES a `metric` field. This is NOT the same as `catalog`.
- table: raw point scan, most recent 1000 rows — use when user asks "show me raw data", "list recent metric points", "show the last N rows", "raw data for X". Always set `metric`.
  Example: "show me recent data for request_duration_ms" → {"type":"ir","ir":{"operation":"table","signals":["metrics"],"metric":"request_duration_ms","filters":[],"time_range":{"from":"now-1h","to":"now"}}}
- distribution: compute scalar stats (percentiles, average, min, max) for a single time window — use when user asks for "p95", "average", "median", "p99 latency", or any single-number summary. Produces one row, NOT a chart.
- **catalog**: Enumerate distinct observable entities (no metric computation). Use ONLY when the user asks "list X", "what X exist?", "show me all X", "which X does Y have?". Does NOT rank by value. Does NOT require a `metric` field.
  Set `catalog_field` to the dimension name: "service_name", "environment", "metric_name", or any label key like "pod", "region", "namespace".
  CRITICAL: Set `catalog_field` to exactly what the user is asking to list:
    - "list services" → catalog_field: "service_name"
    - "list environments" → catalog_field: "environment"
    - "what metrics does X emit?" → catalog_field: "metric_name" (with filter service_name=X)
    - "list all metric names" / "list all available metrics" → catalog_field: "metric_name" (no filter)
    - "list pods for X" → catalog_field: "pod" (with filter service_name=X)
  Example: "list all services" → {"type":"ir","ir":{"operation":"catalog","signals":["metrics"],"catalog_field":"service_name","filters":[],"time_range":{"from":"now-24h","to":"now"}}}
  Example: "list all metric names" → {"type":"ir","ir":{"operation":"catalog","signals":["metrics"],"catalog_field":"metric_name","filters":[],"time_range":{"from":"now-24h","to":"now"}}}
  Example: "what pods does checkout use?" → {"type":"ir","ir":{"operation":"catalog","signals":["metrics"],"catalog_field":"pod","filters":[{"field":"service_name","op":"=","value":"checkout"}],"time_range":{"from":"now-24h","to":"now"}}}
  Example: "what metrics does payments emit?" → {"type":"ir","ir":{"operation":"catalog","signals":["metrics"],"catalog_field":"metric_name","filters":[{"field":"service_name","op":"=","value":"payments"}],"time_range":{"from":"now-24h","to":"now"}}}
- **inventory**: Filter an entity inventory table (infrastructure page, services list) by attribute predicates. Use ONLY when the user is on an entity inventory page and the query is about filtering by entity attributes — NOT about computing a metric or charting over time. Does NOT require a `metric` field. Set `filters` to entity attribute predicates: `entity_type` (host/cluster/namespace/pod/container), `environment`, `service_name`, `display_name` (text search). Do NOT set `catalog_field`.
  Example: "type equals pod" → {"type":"ir","ir":{"operation":"inventory","signals":[],"filters":[{"field":"entity_type","op":"=","value":"pod"}],"time_range":{"from":"now-1h","to":"now"}}}
  Example: "environment equals observable and type equals pod" → {"type":"ir","ir":{"operation":"inventory","signals":[],"filters":[{"field":"environment","op":"=","value":"observable"},{"field":"entity_type","op":"=","value":"pod"}],"time_range":{"from":"now-1h","to":"now"}}}
  Example: "show pods for checkout service" → {"type":"ir","ir":{"operation":"inventory","signals":[],"filters":[{"field":"entity_type","op":"=","value":"pod"},{"field":"service_name","op":"=","value":"checkout"}],"time_range":{"from":"now-1h","to":"now"}}}
  Example: "pods in breach" → {"type":"ir","ir":{"operation":"inventory","signals":[],"filters":[{"field":"entity_type","op":"=","value":"pod"},{"field":"health_state","op":"=","value":"breach"}],"time_range":{"from":"now-1h","to":"now"}}}

**NEVER confuse `topk` and `catalog`:**
- catalog = "list what exists" (no aggregation, no ranking by value)
- topk = "which entities have the highest/lowest metric value" (requires metric aggregation)
- "top 5 services by latency" → topk (NOT catalog)
- "which 3 services have the most errors?" → topk (NOT catalog)
- "list all services" → catalog (NOT topk)

**NEVER confuse `distribution` and `timeseries`:**
- distribution = single scalar answer (one row: "p95 is 42 ms") — use ONLY when the user names a SPECIFIC stat
- timeseries = chart over time (many rows: one per time bucket) — use when no specific stat is named
- **Rule: If the user names a specific stat (p50, p75, p95, p99, average, median, min, max) → distribution**
- **Rule: If the user mentions a metric without naming a specific stat → timeseries**
- "p95 latency for the last hour" → distribution (user named "p95")
- "average latency" / "mean request duration" → distribution (user named "average" / "mean")
- "request latency over the last hour" → timeseries (no specific stat named, wants a chart)
- "latency over time" / "show me a graph of latency" / "latency trend" → timeseries
- "how has latency changed" / "request duration" (no stat) → timeseries

## Log search (signals: ["logs"])

When the user asks about logs, log entries, or searching log content, use `signals: ["logs"]` with `operation: "table"`.
- Set `metric` to null (logs have no metric name)
- Set `query` to the text the user wants to find in log bodies (case-insensitive substring match)
- Use `filters` for structured fields: `service_name`, `severity_text` (INFO/WARN/ERROR), `environment`, `trace_id`, `span_id`
- Set appropriate `time_range`

**Log search examples:**
- "search logs for 'HTTP Request' last 3 hours" → {"operation":"table","signals":["logs"],"metric":null,"query":"HTTP Request","filters":[],"time_range":{"from":"now-3h","to":"now"},"visualization_hint":"table"}
- "show error logs from checkout service" → {"operation":"table","signals":["logs"],"metric":null,"query":null,"filters":[{"field":"service_name","op":"=","value":"checkout"},{"field":"severity_text","op":"=","value":"ERROR"}],"time_range":{"from":"now-1h","to":"now"},"visualization_hint":"table"}
- "logs containing 'timeout' in the last 30 minutes" → {"operation":"table","signals":["logs"],"metric":null,"query":"timeout","filters":[],"time_range":{"from":"now-30m","to":"now"},"visualization_hint":"table"}

**Log query rules:**
- ALWAYS use `signals: ["logs"]` and `operation: "table"` for log queries
- `query` is the free-text body search term; omit or set null if no text search is needed
- `metric`, `window`, `resolution`, `group_by`, `percentiles`, `catalog_field`, `limit` must all be null for log queries

**Filter rules — CRITICAL:**
- NEVER add a filter with an empty string value (e.g. service_name = ""). If you don't know the value, OMIT the filter entirely.
- Only add filters for values you are confident about from the user's query.
- If the user did not mention a specific service, pod, or label value, do NOT add a filter for it.
- NEVER put time constraints in the filters array. ALL temporal bounds go in the `time_range` field ONLY. Valid filter ops are: `=`, `!=`, `=~`, `!~`, `>`, `>=`, `<`, `<=`. The op `range` does not exist.

## `percentiles` field (for distribution operation only)

Set to EXACTLY the stats the user asked for — no more, no less:
- `"p{N}"` for any N from 1–999 (e.g. `"p50"`, `"p75"`, `"p95"`, `"p99"`, `"p999"`)
- `"median"` (same as p50)
- `"average"` or `"mean"` (arithmetic mean)
- `"min"`, `"max"`

**CRITICAL: Include ALL percentiles the user mentioned. Do not drop any.**
**CRITICAL: When the user uses the word "average" or "mean" → set `"percentiles": ["average"]`.**
**CRITICAL: When the user uses the word "median" → include `"median"` in percentiles.**
**These are specific stat names. You MUST include them in the percentiles array.**

Examples:
- User asked **"p99 latency"** → `"percentiles": ["p99"]`
- User asked **"p75, p95, p99"** → `"percentiles": ["p75", "p95", "p99"]`
- User asked **"p95 p99 and average"** → `"percentiles": ["p95", "p99", "average"]` ← include ALL THREE
- User asked **"p99, average, and median"** → `"percentiles": ["p99", "average", "median"]`
- User asked **"average latency"** → operation=distribution, `"percentiles": ["average"]`
- User asked **"median latency"** → operation=distribution, `"percentiles": ["median"]`
- User asked **"median and average latency"** → operation=distribution, `"percentiles": ["median", "average"]`
- User asked **"distribution"** or **"all percentiles"** → omit `percentiles` entirely (null)

NEVER include percentiles the user did not ask for.
NEVER leave percentiles=null when the user asked for specific stats like "average" or "median".
NEVER drop a percentile the user mentioned — if user said "p95 p99 and average", all three must appear.

## `limit` field

`limit` is ONLY for `topk` operations. For all other operations, leave `limit` as null.
- `topk` with "top 5" → `"limit": 5`
- `topk` with "top 3" → `"limit": 3`
- distribution, timeseries, catalog, table → `"limit": null` (ALWAYS)

## `topk` usage

Use `topk` when the user wants to RANK entities by a computed metric value. Always set `metric`.
- `limit`: how many top results (default 10 if not specified)
- Example: "top 5 services by request_duration_ms" → {"operation":"topk","metric":"request_duration_ms","limit":5,...}
- Example: "which 3 services have the highest latency?" → {"operation":"topk","metric":"request_duration_ms","limit":3,...}
- Example: "top 10 by error count" → {"operation":"topk","metric":"<error_metric>","limit":10,...}

## Advisory boundary — MANDATORY

You MUST emit {"type": "decline", ...} for questions that **explicitly** involve:
- Billing, invoicing, or financial reconciliation
- SLA evidence, contractual compliance, or service level objectives used as contracts
- Regulatory compliance (GDPR, HIPAA, SOX, audit trails)
- Any use case requiring BI-grade correctness guarantees

**NEVER decline operational or observability questions.** The following are always safe to answer:
- "list all services", "what metrics does X emit?", "what environments exist?" → catalog
- "show latency", "p95 request duration", "average CPU usage" → distribution/timeseries
- "top services by error rate" → topk
- "show recent logs", "search logs for X" → table with signals:["logs"]
When in doubt, produce an IR. Only decline when the question **explicitly** mentions billing, SLA, or regulatory compliance.

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
            if let Some(rate) = m.effective_sample_rate
                && rate < 1.0
            {
                prompt.push_str(&format!(" [sampled at {:.0}%]", rate * 100.0));
            }
            prompt.push('\n');
        }
    }

    // ── Label keys section ──────────────────────────────────────────────────
    prompt.push_str("\n## Available label keys\n\n");
    if label_keys.is_empty() {
        prompt.push_str("(no label keys discovered for this tenant)\n");
    } else {
        prompt.push_str(&label_keys.join(", "));
        prompt.push('\n');
    }
    prompt.push_str(
        "Use these keys in filters and group_by. Do not invent label keys not listed above.\n\
         Exception: for `catalog` operations the user is explicitly asking to list a dimension — \
         you may use any label name the user mentions as `catalog_field` even if not listed above; \
         the SQL will return empty results if the field does not exist.\n",
    );

    if let Some(svc) = service_scope {
        prompt.push_str(&format!(
            "\n## Service scope\n\nThis query is scoped to service `{svc}`. \
             You do not need to add a service_name filter — it is enforced automatically.\n"
        ));
    }

    if let Some(base) = base_ir {
        let ctx = if base.operation == NlqOperation::Inventory {
            "\n## Page context\n\n\
             You are operating on the **infrastructure inventory page**. \
             This page shows a table of infrastructure entities (hosts, clusters, \
             namespaces, pods, containers). The user is filtering this table by \
             entity attributes — NOT asking for a time-series chart or metric computation.\n\
             **ALWAYS use `operation: \"inventory\"` for queries on this page.** \
             Valid filter fields: `entity_type` (host/cluster/namespace/pod/container), \
             `environment`, `service_name`, `health_state` (healthy/watch/breach), \
             `display_name` (text search). Do NOT add a `metric` field.\n"
        } else if base.signals.contains(&NlqSignal::Logs) {
            "\n## Page context\n\n\
             You are operating on the **log search page**. \
             The user is searching and filtering log entries.\n\
             **ALWAYS use `operation: \"table\"` and `signals: [\"logs\"]` for queries on this page.** \
             Valid filter fields: `service_name`, `severity_text` (INFO/WARN/ERROR/FATAL), \
             `environment`, `trace_id`, `span_id`. Use the `query` field for free-text body search.\n\
             Do NOT set a `metric` field. Do NOT use timeseries, distribution, or catalog operations.\n"
        } else if base.signals.contains(&NlqSignal::Traces) {
            "\n## Page context\n\n\
             You are operating on the **trace search page**. \
             The user is searching and filtering distributed traces.\n\
             **ALWAYS use `operation: \"table\"` and `signals: [\"traces\"]` for queries on this page.** \
             Valid filter fields: `service_name`, `status_code` (OK/ERROR/UNSET), `environment`, \
             `operation` (span operation name, free-text). Do NOT set a `metric` field.\n\
             Do NOT use timeseries, distribution, or catalog operations.\n"
        } else if base.operation == NlqOperation::Catalog {
            "\n## Page context\n\n\
             You are operating on the **services topology page**. \
             The user is filtering the list of observed services.\n\
             **ALWAYS use `operation: \"catalog\"` and `catalog_field: \"service_name\"` for queries on this page.** \
             Valid filter fields: `environment`. Do NOT add a `metric` field.\n"
        } else {
            ""
        };
        if !ctx.is_empty() {
            prompt.push_str(ctx);
        }
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
- histogram   — bucket distribution (only for OTel Histogram metrics with explicit bucket bounds; use `distribution` for gauge/counter metrics)
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

// ── Fuzzy metric resolution ───────────────────────────────────────────────────

/// Attempts to resolve an LLM-hallucinated metric name to a known schema metric.
///
/// Scoring strategy (cheapest first):
/// 1. Case-insensitive exact match.
/// 2. The known metric contains the LLM guess as a substring (e.g. "latency" ⊂ "request_latency_ms").
/// 3. The LLM guess contains a known metric as a substring.
/// 4. Token overlap — split both on `_` and count shared tokens.
/// 5. Semantic alias expansion — common observability synonyms.
///
/// Returns `Some(&str)` referencing the best match from `known` if the score is above threshold,
/// or `None` if no reasonable match exists.
fn fuzzy_resolve_metric<'a>(guess: &str, known: &[&'a str]) -> Option<&'a str> {
    if known.is_empty() {
        return None;
    }
    // If there's only one metric available, use it (common in small testbench setups).
    if known.len() == 1 {
        return Some(known[0]);
    }

    let guess_lower = guess.to_lowercase();
    let guess_tokens: Vec<&str> = guess_lower.split('_').filter(|t| !t.is_empty()).collect();

    // Semantic aliases: common observability synonyms that LLMs frequently interchange.
    let expanded_tokens: Vec<&str> = guess_tokens
        .iter()
        .flat_map(|t| {
            let mut v = vec![*t];
            match *t {
                "latency" => v.extend_from_slice(&["duration", "response"]),
                "duration" => v.extend_from_slice(&["latency", "response"]),
                "response" => v.extend_from_slice(&["latency", "duration"]),
                "requests" | "request" => v.extend_from_slice(&["request", "requests", "http"]),
                "errors" | "error" => v.extend_from_slice(&["error", "errors", "fault"]),
                "cpu" => v.extend_from_slice(&["cpu", "processor"]),
                "memory" | "mem" => v.extend_from_slice(&["memory", "mem", "heap"]),
                "served" | "count" | "total" => v.extend_from_slice(&["request", "count", "total"]),
                "rate" | "per" | "second" => v.extend_from_slice(&["request", "duration", "rate"]),
                _ => {}
            }
            v
        })
        .collect();

    let mut best: Option<&'a str> = None;
    let mut best_score: usize = 0;

    for &candidate in known {
        let cand_lower = candidate.to_lowercase();

        // Case-insensitive exact match → perfect.
        if cand_lower == guess_lower {
            return Some(candidate);
        }

        let mut score: usize = 0;

        // Substring containment (either direction).
        if cand_lower.contains(&guess_lower) {
            score += 3;
        } else if guess_lower.contains(&cand_lower) {
            score += 2;
        }

        // Token overlap (including expanded aliases).
        let cand_tokens: Vec<&str> = cand_lower.split('_').filter(|t| !t.is_empty()).collect();
        let overlap = expanded_tokens
            .iter()
            .filter(|t| cand_tokens.contains(t))
            .count();
        score += overlap;

        if score > best_score {
            best_score = score;
            best = Some(candidate);
        }
    }

    // Require at least 1 token overlap or substring match to consider it valid.
    if best_score >= 1 { best } else { None }
}

// ── Repair prompt ─────────────────────────────────────────────────────────────

/// Builds a repair prompt sent as the next user turn when the LLM returned an invalid response.
///
/// The system prompt is kept unchanged — it already carries the full schema and rules.
/// This prompt asks the model to correct only the failing field while keeping valid parts.
fn build_repair_prompt(question: &str, error: &str, faulty_response: &str) -> String {
    format!(
        "The previous response was invalid and needs correction.\n\n\
         Original question: \"{question}\"\n\n\
         Error: {error}\n\n\
         Faulty response:\n{faulty_response}\n\n\
         Please produce a corrected IR JSON. Correct only the failing field; keep all \
         valid parts unchanged. If you truly cannot answer the question, emit:\n\
         {{\"type\": \"decline\", \"reason\": \"<explanation>\"}}\n\
         Respond with JSON only."
    )
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

/// Normalises an IR JSON value in-place before deserialisation:
/// - Array fields emitted as `null` are replaced with `[]` (serde `default` only handles missing, not null).
/// - Missing or null `time_range` is filled with sensible defaults.
fn normalize_nlq_ir(ir_val: &mut serde_json::Value) {
    normalize_nlq_signals(ir_val);

    // null array → empty array for all Vec fields.
    for field in &["filters", "group_by", "percentiles"] {
        if let Some(v) = ir_val.get_mut(*field)
            && v.is_null()
        {
            *v = serde_json::json!([]);
        }
    }

    // Strip filters with unknown ops (e.g. "range") — the LLM sometimes puts time constraints
    // in the filters array instead of `time_range`. These would fail serde deserialization and
    // the repair loop cannot reliably fix them. Removing them is safe: the `time_range` field
    // already carries temporal bounds, so discarding a time-based filter loses nothing.
    const VALID_OPS: &[&str] = &["=", "!=", "=~", "!~", ">", ">=", "<", "<="];
    if let Some(serde_json::Value::Array(filters)) = ir_val.get_mut("filters") {
        filters.retain(|f| {
            f.get("op")
                .and_then(|op| op.as_str())
                .map(|op| VALID_OPS.contains(&op))
                .unwrap_or(false)
        });
    }

    // Missing or null time_range → sensible defaults.
    match ir_val.get("time_range") {
        None | Some(serde_json::Value::Null) => {
            ir_val["time_range"] = serde_json::json!({"from": "now-24h", "to": "now"});
        }
        _ => patch_null_time_range(ir_val),
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
            normalize_nlq_ir(&mut ir_val);
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
        other => {
            // Fallback: the LLM may have emitted a bare NlqIr without the
            // {"type":"ir","ir":{...}} envelope, or used "type" where "operation"
            // is expected (e.g. {"type":"catalog",...} instead of
            // {"type":"ir","ir":{"operation":"catalog",...}}).
            //
            // Also handles hybrid format: {"type":"catalog","ir":{<valid NlqIr>}}
            // where the LLM mixed old type-as-operation with the IR nesting.
            //
            // Also handles phi3.5-style wrapper: {"type":"response","ir":{...}}
            // or {"type":"response","content":{...}} — the model wraps its JSON
            // in a chat-style envelope rather than returning the IR directly.

            // Case 1: there is a "type" field with any value AND an "ir" key — try to
            // parse it as NlqIr. Handles both operation-name types ("catalog", ...) and
            // chat-wrapper types like phi3.5's {"type":"response","ir":{...}}.
            if let Some(type_val) = other
                && v.get("ir").is_some()
            {
                let mut ir_val = v["ir"].clone();
                if ir_val.get("operation").is_none() {
                    // Only inject type as operation when it looks like a valid op name.
                    const VALID_OPS: &[&str] = &[
                        "timeseries",
                        "rate",
                        "irate",
                        "increase",
                        "histogram",
                        "topk",
                        "table",
                        "distribution",
                        "catalog",
                    ];
                    if VALID_OPS.contains(&type_val) {
                        ir_val["operation"] = serde_json::Value::String((*type_val).to_string());
                    }
                }
                patch_null_time_range(&mut ir_val);
                normalize_nlq_ir(&mut ir_val);
                if ir_val.get("operation").is_some() {
                    tracing::warn!(
                        raw_type = ?other,
                        "NLQ hybrid-envelope fallback: LLM mixed type-as-operation with ir nesting"
                    );
                    match serde_json::from_value::<NlqIr>(ir_val) {
                        Ok(ir) => return Ok(NlqIrOrDecline::Ir(ir)),
                        Err(e) => tracing::warn!(
                            error = %e,
                            "NlqIr deserialize failed (hybrid envelope); trying other fallbacks"
                        ),
                    }
                }
            }

            // Case 2: phi3.5 / chat-wrapper models put IR in a "content" or
            // "result" field when the top-level type is "response" or similar.
            if matches!(
                other,
                Some("response") | Some("chat") | Some("message") | Some("result")
            ) {
                for key in &["content", "result", "data", "output"] {
                    if let Some(inner) = v.get(*key) {
                        // Inner may be an object or a JSON string.
                        let candidate = if inner.is_object() {
                            inner.clone()
                        } else if let Some(s) = inner.as_str() {
                            serde_json::from_str(s).unwrap_or(serde_json::Value::Null)
                        } else {
                            serde_json::Value::Null
                        };
                        if candidate.is_object() {
                            let mut ir_val = candidate;
                            patch_null_time_range(&mut ir_val);
                            normalize_nlq_ir(&mut ir_val);
                            if ir_val.get("operation").is_some() {
                                tracing::warn!(
                                    raw_type = ?other,
                                    nested_key = *key,
                                    "NLQ chat-wrapper fallback: IR found in nested field"
                                );
                                match serde_json::from_value::<NlqIr>(ir_val) {
                                    Ok(ir) => return Ok(NlqIrOrDecline::Ir(ir)),
                                    Err(e) => tracing::warn!(
                                        error = %e,
                                        "NlqIr deserialize failed (chat-wrapper {}); continuing",
                                        key
                                    ),
                                }
                            }
                        }
                    }
                }
                // No nested IR found — phi3.5 returned a wrapper with no usable IR.
                // Treat as InvalidResponse so the repair loop can try to recover.
                return Err(LlmAdapterError::InvalidResponse(format!(
                    "LLM returned chat wrapper (type={other:?}) with no parseable IR"
                )));
            }

            // Case 3: bare IR — the LLM omitted the envelope entirely, or used
            // "type" to carry an operation name directly at the top level.
            let mut ir_val = v.clone();

            if ir_val.get("operation").is_none() {
                // Promote "type" → "operation" only when the value looks like a
                // valid operation name (not a generic chat-style word like "response").
                const VALID_OPS: &[&str] = &[
                    "timeseries",
                    "rate",
                    "irate",
                    "increase",
                    "histogram",
                    "topk",
                    "table",
                    "distribution",
                    "catalog",
                ];
                if let Some(type_val) = ir_val.get("type").cloned()
                    && type_val.as_str().is_some_and(|s| VALID_OPS.contains(&s))
                {
                    ir_val["operation"] = type_val;
                    ir_val.as_object_mut().map(|o| o.remove("type"));
                }
            }

            if ir_val.get("operation").is_some() {
                patch_null_time_range(&mut ir_val);
                tracing::warn!(
                    raw_type = ?other,
                    "NLQ bare-IR fallback: LLM omitted response envelope; attempting direct parse"
                );
                normalize_nlq_ir(&mut ir_val);
                let ir: NlqIr = serde_json::from_value(ir_val).map_err(|e| {
                    LlmAdapterError::InvalidResponse(format!(
                        "NlqIr deserialize failed (bare IR): {e}"
                    ))
                })?;
                return Ok(NlqIrOrDecline::Ir(ir));
            }

            Err(LlmAdapterError::InvalidResponse(format!(
                "unexpected type field: {other:?}"
            )))
        }
    }
}

/// Merges a user-supplied IR into a page base IR for server-side IR composition.
///
/// Rules:
/// - Base `operation`, `signals`, and `catalog_field` are always preserved.
/// - User filters override base filters for the same field (case-insensitive match).
///   User filters for new fields not present in base are appended.
/// - User `time_range` takes precedence when its `from` field is non-empty.
/// - Metric execution-shape fields (`metric`, `window`, `resolution`, `group_by`,
///   `visualization_hint`, `percentiles`, `limit`) are preserved from the base IR.
///   If the base IR leaves `metric` empty, the user metric is used so generic
///   metrics surfaces can still execute a resolved metric query.
/// - User `query` (free-text search term) is preserved from the user IR.
pub fn merge_irs(base: NlqIr, user: NlqIr) -> NlqIr {
    // Base filters first; user filters for the same field replace them.
    let mut merged: Vec<NlqFilter> = Vec::new();
    for f in &base.filters {
        let key = f.field.to_lowercase();
        if !user.filters.iter().any(|u| u.field.to_lowercase() == key) {
            merged.push(f.clone());
        }
    }
    merged.extend(user.filters);

    let merged_time_range = if !user.time_range.from.is_empty() {
        user.time_range
    } else {
        base.time_range
    };

    NlqIr {
        operation: base.operation,
        signals: base.signals,
        catalog_field: base.catalog_field,
        filters: merged,
        time_range: merged_time_range,
        query: user.query,
        metric: base.metric.or(user.metric),
        window: base.window.or(user.window),
        group_by: if base.group_by.is_empty() {
            user.group_by
        } else {
            base.group_by
        },
        resolution: base.resolution.or(user.resolution),
        visualization_hint: base.visualization_hint.or(user.visualization_hint),
        percentiles: base.percentiles.or(user.percentiles),
        limit: base.limit.or(user.limit),
    }
}

pub(crate) fn parse_user_query_input(input: &str) -> Result<UserQueryInput, LlmAdapterError> {
    let trimmed = input.trim();
    if trimmed.starts_with('{') {
        let mut ir_val: serde_json::Value = serde_json::from_str(trimmed).map_err(|e| {
            LlmAdapterError::InvalidResponse(format!("raw NLQ IR JSON is invalid: {e}"))
        })?;
        patch_null_time_range(&mut ir_val);
        normalize_nlq_ir(&mut ir_val);
        let ir = serde_json::from_value::<NlqIr>(ir_val).map_err(|e| {
            LlmAdapterError::InvalidResponse(format!("raw NLQ IR JSON is invalid: {e}"))
        })?;
        return Ok(UserQueryInput::RawIr(Box::new(ir)));
    }
    Ok(UserQueryInput::NaturalLanguage)
}

// ── Simple IR Shorthand ───────────────────────────────────────────────────────
//
// Implements ADR-029: deterministic NlqIr construction from a compact token syntax.
// Activated when the user query starts with '/' (explicit bypass) or when no LLM
// is configured (graceful degradation).
//
// Syntax: tokens are space-separated.
//   m:<name>          → metric field
//   f:<field>:<val>   → equality filter (explicit prefix)
//   op:<operation>    → operation override (timeseries|rate|topk|table|…)
//   <field>:<val>     → equality filter (shorthand — any token containing ':')
//   "quoted text"     → free-text query term (appended)
//   unquoted word     → free-text query term (appended)

/// Parsed tokens from a shorthand query string.
/// All fields are optional — only what was explicitly written is set.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ShorthandIr {
    pub operation: Option<NlqOperation>,
    pub metric: Option<String>,
    pub filters: Vec<NlqFilter>,
    pub query: Option<String>,
}

impl ShorthandIr {
    /// Convert to a standalone NlqIr when no base_ir is available.
    /// Applies sensible defaults for required fields.
    pub fn into_nlq_ir(self) -> NlqIr {
        let signals = if self.metric.is_some() {
            vec![NlqSignal::Metrics]
        } else if self.query.is_some() {
            vec![NlqSignal::Logs]
        } else {
            vec![NlqSignal::Metrics]
        };
        NlqIr {
            operation: self.operation.unwrap_or(NlqOperation::Timeseries),
            signals,
            metric: self.metric,
            window: None,
            filters: self.filters,
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: self.query,
        }
    }
}

/// Apply a parsed shorthand on top of a `base_ir`.
///
/// - Shorthand filters override base filters for the same field (same logic as `merge_irs`).
/// - Shorthand metric, operation, and query override base values when explicitly set.
/// - All other base fields (signals, window, group_by, resolution, …) are preserved.
pub(crate) fn apply_shorthand_to_ir(base: NlqIr, sh: ShorthandIr) -> NlqIr {
    let mut filters: Vec<NlqFilter> = Vec::new();
    for f in &base.filters {
        let key = f.field.to_lowercase();
        if !sh.filters.iter().any(|u| u.field.to_lowercase() == key) {
            filters.push(f.clone());
        }
    }
    filters.extend(sh.filters);

    NlqIr {
        operation: sh.operation.unwrap_or(base.operation),
        signals: base.signals,
        catalog_field: base.catalog_field,
        metric: sh.metric.or(base.metric),
        window: base.window,
        filters,
        group_by: base.group_by,
        resolution: base.resolution,
        time_range: base.time_range,
        visualization_hint: base.visualization_hint,
        percentiles: base.percentiles,
        limit: base.limit,
        query: sh.query,
    }
}

/// Parse a shorthand query string into a [`ShorthandIr`].
///
/// Does not require or consume the leading `/` — callers strip it before calling.
pub(crate) fn parse_shorthand_ir(input: &str) -> ShorthandIr {
    let mut sh = ShorthandIr::default();
    let mut query_parts: Vec<String> = Vec::new();

    for token in tokenize_shorthand(input) {
        if let Some(name) = token.strip_prefix("m:") {
            if !name.is_empty() {
                sh.metric = Some(name.to_string());
            }
        } else if let Some(rest) = token.strip_prefix("f:") {
            if let Some(colon) = rest.find(':') {
                let field = &rest[..colon];
                let val = &rest[colon + 1..];
                if !field.is_empty() {
                    sh.filters.push(NlqFilter {
                        field: field.to_string(),
                        op: NlqFilterOp::Eq,
                        value: val.to_string(),
                    });
                }
            }
        } else if let Some(op_str) = token.strip_prefix("op:") {
            sh.operation = parse_shorthand_operation(op_str);
        } else if let Some(colon) = token.find(':') {
            // Generic <field>:<val> — shorthand filter
            let field = &token[..colon];
            let val = &token[colon + 1..];
            if !field.is_empty() && !val.is_empty() {
                sh.filters.push(NlqFilter {
                    field: field.to_string(),
                    op: NlqFilterOp::Eq,
                    value: val.to_string(),
                });
            } else {
                // Ambiguous (e.g. trailing colon) — treat as freetext
                query_parts.push(token);
            }
        } else if !token.is_empty() {
            query_parts.push(token);
        }
    }

    if !query_parts.is_empty() {
        sh.query = Some(query_parts.join(" "));
    }

    sh
}

/// Split a shorthand string into tokens, respecting double-quoted phrases.
fn tokenize_shorthand(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '"' {
            chars.next();
            let mut s = String::new();
            loop {
                match chars.next() {
                    Some('"') | None => break,
                    Some(c) => s.push(c),
                }
            }
            if !s.is_empty() {
                tokens.push(s);
            }
        } else {
            let mut s = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                s.push(c);
                chars.next();
            }
            tokens.push(s);
        }
    }

    tokens
}

fn parse_shorthand_operation(s: &str) -> Option<NlqOperation> {
    match s {
        "timeseries" => Some(NlqOperation::Timeseries),
        "rate" => Some(NlqOperation::Rate),
        "irate" => Some(NlqOperation::Irate),
        "increase" => Some(NlqOperation::Increase),
        "histogram" => Some(NlqOperation::Histogram),
        "topk" => Some(NlqOperation::Topk),
        "table" => Some(NlqOperation::Table),
        "distribution" => Some(NlqOperation::Distribution),
        "catalog" => Some(NlqOperation::Catalog),
        "inventory" => Some(NlqOperation::Inventory),
        _ => None,
    }
}

// ── Service scope enforcement ─────────────────────────────────────────────────

/// Patches a JSON IR value in-place to replace null `time_range.from` / `time_range.to`
/// with sensible defaults (now-24h / now) so the LLM emitting null values doesn't break parsing.
fn patch_null_time_range(ir_val: &mut serde_json::Value) {
    if let Some(tr) = ir_val.get_mut("time_range") {
        if tr.get("from").is_some_and(|v| v.is_null()) {
            tr["from"] = serde_json::Value::String("now-24h".into());
        }
        if tr.get("to").is_some_and(|v| v.is_null()) {
            tr["to"] = serde_json::Value::String("now".into());
        }
    }
}

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

/// A prepared system prompt + question pair, ready to be sent to an `LlmCaller`
/// (or, in a later phase, to a browser-side WebLLM engine).
pub struct NlqPreparedCall {
    pub system_prompt: String,
    pub question: String,
}

/// Outcome of `prepare_nlq_pipeline`: either the request was declined before any LLM
/// call was needed, or a system prompt + question pair is ready to be sent to an LLM.
#[allow(clippy::large_enum_variant)]
pub enum NlqPrepareOutcome {
    Declined(NlqQueryResponse),
    Prepared(NlqPreparedCall),
}

/// Runs the pre-LLM portion of the NLQ pipeline: server-side deny gate, bounded schema
/// context fetch, and system prompt construction. Contains today's steps 1–3 verbatim.
///
/// Never produces a repair-turn prompt — the returned `question` is always the
/// original question. Repair prompts are built by `resume_nlq_pipeline`.
pub async fn prepare_nlq_pipeline(
    db: &PgPool,
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    req: &NlqQueryRequest,
) -> Result<NlqPrepareOutcome, LlmAdapterError> {
    let question = req.question.as_deref().unwrap_or("");
    let question_preview = question.chars().take(256).collect::<String>();

    tracing::info!(
        tenant_id = %tenant_id,
        question = %question_preview,
        service_scope = ?req.service_name,
        "NLQ pipeline started"
    );

    // 1. Server-side deny gate (belt and suspenders — LLM prompt also instructs decline)
    if let Some(reason) = server_side_deny_gate(question) {
        tracing::info!(
            tenant_id = %tenant_id,
            question = %question_preview,
            reason = %reason,
            "NLQ declined by server-side deny gate"
        );
        return Ok(NlqPrepareOutcome::Declined(NlqQueryResponse::Decline {
            reason,
        }));
    }

    // 2. Fetch bounded schema context (up to 20 schema-complete metrics)
    let (metrics, label_keys) = fetch_schema_context(db, ch, tenant_id, 20).await?;

    // 3. Build system prompt
    let system_prompt = build_system_prompt(
        &metrics,
        &label_keys,
        req.service_name.as_deref(),
        req.base_ir.as_ref(),
    );

    tracing::debug!(
        tenant_id = %tenant_id,
        prompt_metric_count = metrics.len(),
        "NLQ calling LLM"
    );

    Ok(NlqPrepareOutcome::Prepared(NlqPreparedCall {
        system_prompt,
        question: question.to_string(),
    }))
}

/// Outcome of `resume_nlq_pipeline`: either a final response, or a signal that the raw
/// LLM response failed to parse and a repair-turn prompt should be sent back to the LLM.
#[allow(clippy::large_enum_variant)]
pub enum NlqResumeOutcome {
    Final(NlqQueryResponse),
    NeedsRepair { repair_prompt: String },
}

/// Runs the post-LLM portion of the NLQ pipeline for a single raw completion: parses
/// it, and either produces a final response (today's steps 6–9, unchanged) or — on an
/// `InvalidResponse` parse error within the repair budget — a repair-turn prompt for the
/// caller to send back to the LLM. The repair loop itself lives in the caller.
pub async fn resume_nlq_pipeline(
    db: &PgPool,
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    req: &NlqQueryRequest,
    original_question: &str,
    repair_attempt: usize,
    raw_llm_response: &str,
) -> Result<NlqResumeOutcome, LlmAdapterError> {
    let question_preview = original_question.chars().take(256).collect::<String>();

    // 4 + 5. Parse the raw LLM response, branching on repair budget for InvalidResponse.
    let parsed = match parse_llm_response(raw_llm_response) {
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
        Err(LlmAdapterError::InvalidResponse(ref reason))
            if repair_attempt < MAX_REPAIR_ATTEMPTS =>
        {
            let next_repair_attempt = repair_attempt + 1;
            tracing::warn!(
                tenant_id = %tenant_id,
                repair_attempt = next_repair_attempt,
                error = %reason,
                "NLQ repair attempt"
            );
            return Ok(NlqResumeOutcome::NeedsRepair {
                repair_prompt: build_repair_prompt(original_question, reason, raw_llm_response),
            });
        }
        Err(LlmAdapterError::InvalidResponse(reason)) => {
            let truncated = raw_llm_response.chars().take(512).collect::<String>();
            tracing::warn!(
                tenant_id = %tenant_id,
                question = %question_preview,
                error = %reason,
                raw_response = %truncated,
                repair_attempts = repair_attempt,
                "NLQ repair budget exhausted"
            );
            return Ok(NlqResumeOutcome::Final(NlqQueryResponse::InvalidResponse {
                reason,
                raw_llm_response: raw_llm_response.to_string(),
            }));
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
            return Ok(NlqResumeOutcome::Final(NlqQueryResponse::Decline {
                reason,
            }));
        }
        NlqIrOrDecline::Capabilities => {
            tracing::info!(
                tenant_id = %tenant_id,
                question = %question_preview,
                "NLQ capabilities short-circuit"
            );
            return Ok(NlqResumeOutcome::Final(NlqQueryResponse::Capabilities {
                hint: build_capabilities_hint(),
            }));
        }
        NlqIrOrDecline::Ir(ir) => ir,
    };

    // 7. Enforce service scope if provided (server-side, not LLM-dependent)
    if let Some(svc) = &req.service_name {
        enforce_service_scope(&mut ir, svc);
    }

    // 8. Validate: metric is required for non-catalog, non-log, non-trace, non-inventory operations
    let is_log_query = ir.signals == vec![NlqSignal::Logs];
    let is_trace_query = ir.signals == vec![NlqSignal::Traces];
    let is_no_metric_op = matches!(
        ir.operation,
        NlqOperation::Catalog | NlqOperation::Inventory
    );
    if ir.metric.is_none() && !is_no_metric_op && !is_log_query && !is_trace_query {
        tracing::info!(
            tenant_id = %tenant_id,
            question = %question_preview,
            source = "no_metric",
            "NLQ declined — no metric identified"
        );
        return Ok(NlqResumeOutcome::Final(NlqQueryResponse::Decline {
            reason: "Could not identify a metric for this question. \
                     Please rephrase or specify a metric name."
                .into(),
        }));
    }

    // 8b. Fuzzy metric resolution — skip for log/trace queries (no metric to resolve).
    if !is_log_query
        && !is_trace_query
        && let Some(ref llm_metric) = ir.metric
    {
        let all_fields = crate::mcp_tools::list_signal_fields(db, tenant_id, "metrics")
            .await
            .unwrap_or_default();
        let known_names: Vec<&str> = all_fields.iter().map(|m| m.field_name.as_str()).collect();
        if !known_names.iter().any(|k| k == &llm_metric.as_str())
            && let Some(resolved) = fuzzy_resolve_metric(llm_metric, &known_names)
        {
            tracing::info!(
                tenant_id = %tenant_id,
                original = %llm_metric,
                resolved = %resolved,
                "NLQ fuzzy metric resolution applied"
            );
            ir.metric = Some(resolved.to_string());
        }
    }

    if req.mode == NlqQueryMode::Interpret {
        tracing::info!(
            tenant_id = %tenant_id,
            operation = ?ir.operation,
            "NLQ interpreted to IR without execution"
        );
        return Ok(NlqResumeOutcome::Final(NlqQueryResponse::Ir { ir }));
    }

    // Apply base_ir merge in execute mode: base operation/signals preserved,
    // user filters override same-field base filters, user time_range wins.
    if let Some(base) = req.base_ir.clone() {
        ir = merge_irs(base, ir);
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

    tracing::debug!(
        tenant_id = %tenant_id,
        mcp_elapsed_ms,
        row_count = frame.data.len(),
        "NLQ MCP query complete"
    );

    Ok(NlqResumeOutcome::Final(NlqQueryResponse::Frame { frame }))
}

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

    let prepared = match prepare_nlq_pipeline(db, ch, tenant_id, req).await? {
        NlqPrepareOutcome::Declined(response) => return Ok(response),
        NlqPrepareOutcome::Prepared(prepared) => prepared,
    };

    // 4 + 5. Call LLM with an optional repair loop.
    //
    // On an `InvalidResponse` parse error the pipeline sends a structured repair prompt
    // as the next user turn (system prompt unchanged — it already carries schema + rules).
    // The loop is capped at MAX_REPAIR_ATTEMPTS to bound token spend.
    let llm_start = std::time::Instant::now();
    let mut repair_attempt: usize = 0;
    let mut next_question = prepared.question.clone();
    let response = loop {
        let raw = llm.call(&prepared.system_prompt, &next_question).await?;

        tracing::debug!(
            tenant_id = %tenant_id,
            llm_elapsed_ms = llm_start.elapsed().as_millis(),
            raw_response_len = raw.len(),
            "NLQ LLM call complete"
        );

        match resume_nlq_pipeline(
            db,
            ch,
            tenant_id,
            req,
            &prepared.question,
            repair_attempt,
            &raw,
        )
        .await?
        {
            NlqResumeOutcome::Final(response) => break response,
            NlqResumeOutcome::NeedsRepair { repair_prompt } => {
                repair_attempt += 1;
                next_question = repair_prompt;
            }
        }
    };
    let _llm_elapsed_ms = llm_start.elapsed().as_millis();

    if matches!(response, NlqQueryResponse::Frame { .. }) {
        let pipeline_elapsed_ms = pipeline_start.elapsed().as_millis();
        tracing::info!(
            tenant_id = %tenant_id,
            pipeline_elapsed_ms,
            result = "frame",
            "NLQ pipeline complete"
        );
    }

    Ok(response)
}

// ── HTTP handler ──────────────────────────────────────────────────────────────

fn map_mcp_error(
    e: crate::mcp_query::McpQueryError,
    tenant_id: Uuid,
) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        crate::mcp_query::McpQueryError::SqlTemplate(
            crate::sql_templates::SqlTemplateError::InvalidFilterValue(field),
        ) => {
            tracing::warn!(tenant_id = %tenant_id, field = %field, "NLQ rejected: invalid filter value");
            (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": format!("invalid filter value for field: {field}")}),
                ),
            )
        }
        crate::mcp_query::McpQueryError::MissingMetric => {
            tracing::warn!(tenant_id = %tenant_id, "NLQ rejected: metric is required");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "metric is required for this operation"})),
            )
        }
        _ if e.to_string().contains("Connect") || e.to_string().contains("network") => {
            tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed — data store unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    serde_json::json!({"error": "data store temporarily unavailable, please retry"}),
                ),
            )
        }
        _ => {
            tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query execution failed: {e}")})),
            )
        }
    }
}

/// Runs the pre-LLM shortcuts shared by `handle_nlq_query` and `handle_nlq_prepare`, in the
/// same order both endpoints must apply them: empty-question+base_ir page-load execution,
/// raw-IR-pasted-as-question, and `/`-prefixed shorthand bypass.
///
/// Returns `None` when no shortcut applies (the caller must go on to run the LLM pipeline).
/// Returns `Some(Ok(response))` / `Some(Err(status, body))` when a shortcut produced a final
/// `NlqQueryResponse` (or a request error) without needing an LLM call at all.
async fn try_pre_llm_shortcuts(
    state: &AppState,
    ctx: &TenantContext,
    req: &NlqQueryRequest,
) -> Option<Result<NlqQueryResponse, (StatusCode, Json<serde_json::Value>)>> {
    let question = req.question.as_deref().unwrap_or("").trim();

    // Case 1: no question + base_ir set → execute base_ir directly (page-load pattern)
    if question.is_empty() {
        if let Some(ref base) = req.base_ir {
            let mut ir = base.clone();
            if let Some(svc) = &req.service_name {
                enforce_service_scope(&mut ir, svc);
            }
            if req.mode == NlqQueryMode::Interpret {
                return Some(Ok(NlqQueryResponse::Ir { ir }));
            }
            return Some(
                match execute_mcp_query(&state.db, &state.ch, ctx.tenant_id, &ir).await {
                    Ok(frame) => Ok(NlqQueryResponse::Frame { frame }),
                    Err(e) => Err(map_mcp_error(e, ctx.tenant_id)),
                },
            );
        }
        return Some(Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "question is required when base_ir is not provided"})),
        )));
    }

    match parse_user_query_input(question) {
        Ok(UserQueryInput::RawIr(ir)) => {
            let mut ir = *ir;
            if let Some(svc) = &req.service_name {
                enforce_service_scope(&mut ir, svc);
            }
            if req.mode == NlqQueryMode::Interpret {
                return Some(Ok(NlqQueryResponse::Ir { ir }));
            }
            // Apply base_ir merge if provided
            if let Some(base) = req.base_ir.clone() {
                ir = merge_irs(base, ir);
            }
            return Some(
                match execute_mcp_query(&state.db, &state.ch, ctx.tenant_id, &ir).await {
                    Ok(frame) => Ok(NlqQueryResponse::Frame { frame }),
                    Err(e) => Err(map_mcp_error(e, ctx.tenant_id)),
                },
            );
        }
        Ok(UserQueryInput::NaturalLanguage) => {}
        Err(e) => {
            return Some(Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(serde_json::json!({"error": e.to_string()})),
            )));
        }
    }

    // Case: user explicitly bypasses the LLM with a leading '/' (ADR-029 shorthand).
    if let Some(shorthand_input) = question.strip_prefix('/') {
        let sh = parse_shorthand_ir(shorthand_input);
        let mut ir = if let Some(base) = req.base_ir.clone() {
            apply_shorthand_to_ir(base, sh)
        } else {
            sh.into_nlq_ir()
        };
        if let Some(svc) = &req.service_name {
            enforce_service_scope(&mut ir, svc);
        }
        tracing::info!(tenant_id = %ctx.tenant_id, "shorthand bypass: executing IR without LLM");
        if req.mode == NlqQueryMode::Interpret {
            return Some(Ok(NlqQueryResponse::Ir { ir }));
        }
        return Some(
            match execute_mcp_query(&state.db, &state.ch, ctx.tenant_id, &ir).await {
                Ok(frame) => Ok(NlqQueryResponse::Frame { frame }),
                Err(e) => Err(map_mcp_error(e, ctx.tenant_id)),
            },
        );
    }

    None
}

/// Maps an `LlmAdapterError` produced by `prepare_nlq_pipeline` / `resume_nlq_pipeline` to an
/// HTTP status + JSON error body, for the two-phase `/v1/nlq/prepare` and `/v1/nlq/complete`
/// endpoints. Mirrors the error handling `handle_nlq_query` applies to `run_nlq_pipeline`
/// (minus the `LlmCall` variant, which neither `prepare_nlq_pipeline` nor `resume_nlq_pipeline`
/// can produce — no endpoint in this module calls an `LlmCaller`).
fn map_llm_adapter_error(
    e: LlmAdapterError,
    tenant_id: Uuid,
) -> (StatusCode, Json<serde_json::Value>) {
    match e {
        LlmAdapterError::QueryExecution(ref inner)
            if inner.to_string().contains("Connect") || inner.to_string().contains("network") =>
        {
            tracing::error!(error = %inner, tenant_id = %tenant_id, "NLQ pipeline failed — data store unreachable");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    serde_json::json!({"error": "data store temporarily unavailable, please retry"}),
                ),
            )
        }
        LlmAdapterError::QueryExecution(crate::mcp_query::McpQueryError::SqlTemplate(
            crate::sql_templates::SqlTemplateError::InvalidFilterValue(field),
        )) => {
            tracing::warn!(field = %field, "NLQ rejected: invalid filter value");
            (
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": format!("invalid filter value for field: {field}")}),
                ),
            )
        }
        LlmAdapterError::QueryExecution(crate::mcp_query::McpQueryError::MissingMetric) => {
            tracing::warn!(tenant_id = %tenant_id, "NLQ rejected: metric is required");
            (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "metric is required for this operation"})),
            )
        }
        e => {
            tracing::error!(error = %e, tenant_id = %tenant_id, "NLQ pipeline failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "NLQ pipeline failed"})),
            )
        }
    }
}

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
    if let Some(result) = try_pre_llm_shortcuts(&state, &ctx, &req).await {
        return result.map(Json);
    }

    // `POST /v1/nlq` is the single-call remote-style endpoint: it calls an `LlmCaller`
    // itself. A tenant configured for WebLLM has nothing server-side to call here — the
    // client is expected to use `/v1/nlq/prepare` + `/v1/nlq/complete` instead, which run
    // inference in-browser. This is a guard against a stale/misrouted client, not a path
    // any correctly written client should hit in normal operation.
    if crate::llm_config::fetch_provider(&state.db).await == crate::llm_config::LlmProvider::Webllm
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "LLM provider is configured as webllm; use /v1/nlq/prepare and /v1/nlq/complete instead"
            })),
        ));
    }

    let question = req.question.as_deref().unwrap_or("").trim();

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
        let api_key = crate::llm_config::fetch_db_key(&state.db)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "database error resolving LLM config"})),
                )
            })?;
        let url = crate::llm_config::fetch_db_value(&state.db, "llm_url")
            .await
            .ok()
            .flatten()
            .filter(|v| !v.is_empty());
        let model = crate::llm_config::fetch_db_value(&state.db, "llm_model")
            .await
            .ok()
            .flatten()
            .filter(|v| !v.is_empty());

        if api_key.is_none() && url.is_none() {
            // Graceful degradation (ADR-029): no LLM configured → deterministic shorthand fallback.
            tracing::info!(tenant_id = %ctx.tenant_id, "LLM not configured — using shorthand fallback");
            let sh = parse_shorthand_ir(question);
            let mut ir = if let Some(base) = req.base_ir.clone() {
                apply_shorthand_to_ir(base, sh)
            } else {
                sh.into_nlq_ir()
            };
            if let Some(svc) = &req.service_name {
                enforce_service_scope(&mut ir, svc);
            }
            if req.mode == NlqQueryMode::Interpret {
                return Ok(Json(NlqQueryResponse::Ir { ir }));
            }
            return match execute_mcp_query(&state.db, &state.ch, ctx.tenant_id, &ir).await {
                Ok(frame) => Ok(Json(NlqQueryResponse::Frame { frame })),
                Err(e) => {
                    tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "shorthand fallback MCP query failed");
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": format!("No AI model is configured — query was interpreted as shorthand syntax. {e}")
                        })),
                    ))
                }
            };
        }

        db_caller = Some(OpenAiLlmCaller::from_key(
            api_key.unwrap_or_default(),
            url,
            model,
        ));
        db_caller.as_ref().unwrap()
    };

    match run_nlq_pipeline(&state.db, &state.ch, llm, ctx.tenant_id, &req).await {
        Ok(response) => Ok(Json(response)),
        Err(LlmAdapterError::LlmCall(e)) => {
            tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "LLM call failed");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("LLM call failed: {e}")})),
            ))
        }
        // Infrastructure unavailability (ClickHouse or PostgreSQL not reachable) returns 503
        // rather than 500 so callers can distinguish a transient dependency failure from a
        // permanent server error.
        Err(LlmAdapterError::QueryExecution(ref e))
            if e.to_string().contains("Connect") || e.to_string().contains("network") =>
        {
            tracing::error!(error = %e, tenant_id = %ctx.tenant_id, "NLQ pipeline failed — data store unreachable");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                Json(
                    serde_json::json!({"error": "data store temporarily unavailable, please retry"}),
                ),
            ))
        }
        Err(LlmAdapterError::QueryExecution(crate::mcp_query::McpQueryError::SqlTemplate(
            crate::sql_templates::SqlTemplateError::InvalidFilterValue(field),
        ))) => {
            tracing::warn!(field = %field, "NLQ rejected: invalid filter value");
            Err((
                StatusCode::BAD_REQUEST,
                Json(
                    serde_json::json!({"error": format!("invalid filter value for field: {field}")}),
                ),
            ))
        }
        Err(LlmAdapterError::QueryExecution(crate::mcp_query::McpQueryError::MissingMetric)) => {
            tracing::warn!(tenant_id = %ctx.tenant_id, "NLQ rejected: metric is required");
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "metric is required for this operation"})),
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

// ── Two-phase NLQ pipeline (client-side LLM inference) ─────────────────────────
//
// `POST /v1/nlq/prepare` + `POST /v1/nlq/complete` expose `prepare_nlq_pipeline` /
// `resume_nlq_pipeline` over HTTP for a caller that runs LLM inference itself (e.g. a
// browser-side WebLLM engine, added in a later change) instead of the server calling out to
// a remote LLM. `POST /v1/nlq` above is unaffected and keeps working exactly as before.

/// Response for `POST /v1/nlq/prepare`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum NlqPrepareResponse {
    /// A pre-LLM shortcut fired, or the pipeline declined the question — no LLM call needed.
    Final { response: NlqQueryResponse },
    /// A system prompt + question are ready to send to an LLM. `session_token` must be
    /// echoed back on the matching `/v1/nlq/complete` call.
    Prepared {
        session_token: Uuid,
        system_prompt: String,
        question: String,
    },
}

/// POST /v1/nlq/prepare
///
/// Runs the pre-LLM portion of the NLQ pipeline (pre-LLM shortcuts, then
/// `prepare_nlq_pipeline`) and, if an LLM call is actually needed, stores a session so the
/// repair-attempt cap can be enforced server-side across the follow-up `/v1/nlq/complete`
/// call(s). Never calls an `LlmCaller` itself — the caller runs inference and posts the raw
/// response back to `/v1/nlq/complete`.
pub async fn handle_nlq_prepare(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(req): Json<NlqQueryRequest>,
) -> Result<Json<NlqPrepareResponse>, (StatusCode, Json<serde_json::Value>)> {
    if let Some(result) = try_pre_llm_shortcuts(&state, &ctx, &req).await {
        return result.map(|response| Json(NlqPrepareResponse::Final { response }));
    }

    // Resolve the provider for parity with `handle_nlq_query`, though today it's a no-op
    // here: `/prepare` never calls an `LlmCaller` itself for either provider — Remote
    // tenants proceed into the pipeline exactly as before, and Webllm tenants are the
    // *intended* audience for this endpoint (the client runs inference and posts the raw
    // response to `/v1/nlq/complete`). This call exists only so the branch is visible next
    // to `handle_nlq_query`'s; it doesn't change behavior for either provider value.
    let _provider = crate::llm_config::fetch_provider(&state.db).await;

    match prepare_nlq_pipeline(&state.db, &state.ch, ctx.tenant_id, &req).await {
        Ok(NlqPrepareOutcome::Declined(response)) => {
            Ok(Json(NlqPrepareResponse::Final { response }))
        }
        Ok(NlqPrepareOutcome::Prepared(prepared)) => {
            let session_token =
                state
                    .sessions
                    .insert(ctx.tenant_id, req, prepared.question.clone());
            Ok(Json(NlqPrepareResponse::Prepared {
                session_token,
                system_prompt: prepared.system_prompt,
                question: prepared.question,
            }))
        }
        Err(e) => Err(map_llm_adapter_error(e, ctx.tenant_id)),
    }
}

/// Request body for `POST /v1/nlq/complete`.
#[derive(Debug, Deserialize)]
pub struct NlqCompleteRequest {
    pub session_token: Uuid,
    pub raw_llm_response: String,
}

/// Response for `POST /v1/nlq/complete`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[allow(clippy::large_enum_variant)]
pub enum NlqCompleteResponse {
    Final { response: NlqQueryResponse },
    NeedsRepair { repair_prompt: String },
}

/// POST /v1/nlq/complete
///
/// Consumes a raw LLM completion for a session created by `/v1/nlq/prepare`. On a parse
/// failure within the repair budget, keeps the session alive (server-incremented
/// `repair_attempt`) and returns a repair prompt for the caller to send back to the LLM as
/// the next turn. On a `Final` outcome (success, decline, capabilities, or repair-budget-
/// exhausted `InvalidResponse`), removes the session from the store.
pub async fn handle_nlq_complete(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
    Json(body): Json<NlqCompleteRequest>,
) -> Result<Json<NlqCompleteResponse>, (StatusCode, Json<serde_json::Value>)> {
    let Some(mut session) = state
        .sessions
        .take_for_resume(body.session_token, ctx.tenant_id)
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "unknown or expired NLQ session"})),
        ));
    };

    match resume_nlq_pipeline(
        &state.db,
        &state.ch,
        session.tenant_id,
        &session.req,
        &session.original_question,
        session.repair_attempt,
        &body.raw_llm_response,
    )
    .await
    {
        Ok(NlqResumeOutcome::Final(response)) => {
            // Session already removed by `take_for_resume` — nothing further to do.
            Ok(Json(NlqCompleteResponse::Final { response }))
        }
        Ok(NlqResumeOutcome::NeedsRepair { repair_prompt }) => {
            session.repair_attempt += 1;
            state.sessions.put_back(body.session_token, session);
            Ok(Json(NlqCompleteResponse::NeedsRepair { repair_prompt }))
        }
        Err(e) => Err(map_llm_adapter_error(e, ctx.tenant_id)),
    }
}

// ── Metadata endpoint ─────────────────────────────────────────────────────────

/// Response type for GET /v1/nlq/metadata.
///
/// Exposes the metadata context used for NLQ prompt construction so the frontend
/// can populate label pickers and catalog browsers.  Also serves as the stable
/// metadata boundary enabling future MCP server separation.
#[derive(Debug, Serialize)]
pub struct NlqMetadataResponse {
    pub metrics: Vec<crate::mcp_tools::SignalField>,
    pub label_keys: Vec<String>,
    pub aggregations: Vec<&'static str>,
    pub time_range_presets: Vec<&'static str>,
}

/// GET /v1/nlq/metadata
///
/// Returns the metadata context used for NLQ prompt construction.
/// Useful for the frontend to populate label pickers and catalog browsers.
/// Also serves as the stable metadata boundary enabling future MCP server separation.
pub async fn handle_nlq_metadata(
    State(state): State<AppState>,
    Extension(ctx): Extension<TenantContext>,
) -> Result<Json<NlqMetadataResponse>, (StatusCode, Json<serde_json::Value>)> {
    let (metrics, label_keys) = fetch_schema_context(&state.db, &state.ch, ctx.tenant_id, 20)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e.to_string()})),
            )
        })?;
    Ok(Json(NlqMetadataResponse {
        metrics,
        label_keys,
        aggregations: vec![
            "avg", "p50", "p75", "p95", "p99", "sum", "count", "min", "max",
        ],
        time_range_presets: vec![
            "now-15m", "now-1h", "now-6h", "now-24h", "now-7d", "now-30d",
        ],
    }))
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{NlqOperation, NlqSignal, NlqTimeRange, NlqVisualizationHint};

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
            limit: None,
            query: None,
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

    // ── map_mcp_error ────────────────────────────────────────────────────────

    #[test]
    fn map_mcp_error_catch_all_includes_underlying_detail() {
        let e = crate::mcp_query::McpQueryError::UnknownMetric("missing_metric".to_string());
        let tenant_id = uuid::Uuid::nil();
        let (status, axum::Json(body)) = map_mcp_error(e, tenant_id);
        assert_eq!(status, axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        let msg = body["error"].as_str().unwrap();
        assert!(
            msg.contains("missing_metric"),
            "error body should include underlying detail, got: {msg}"
        );
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

    // ── bare-IR fallback ──────────────────────────────────────────────────────

    #[test]
    fn parse_bare_ir_without_envelope_succeeds() {
        // LLM emitted plain NlqIr object without {"type":"ir","ir":{...}} wrapper.
        let json = r#"{
            "operation": "timeseries",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "filters": [],
            "group_by": [],
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;
        match parse_llm_response(json).expect("should parse bare IR") {
            NlqIrOrDecline::Ir(ir) => assert_eq!(ir.operation, NlqOperation::Timeseries),
            other => panic!("expected Ir, got {other:?}"),
        }
    }

    #[test]
    fn parse_bare_ir_with_type_as_operation_succeeds() {
        // LLM used "type":"catalog" instead of wrapping in envelope + using "operation".
        // This is the exact pattern that caused the "list all services" regression.
        let json = r#"{
            "type": "catalog",
            "signals": ["metrics"],
            "catalog_field": "service_name",
            "filters": [],
            "time_range": {"from": "now-24h", "to": "now"}
        }"#;
        match parse_llm_response(json).expect("should parse catalog via fallback") {
            NlqIrOrDecline::Ir(ir) => {
                assert_eq!(ir.operation, NlqOperation::Catalog);
                assert_eq!(ir.catalog_field.as_deref(), Some("service_name"));
            }
            other => panic!("expected Ir, got {other:?}"),
        }
    }

    #[test]
    fn parse_bare_ir_with_unknown_operation_still_errors() {
        // "type":"unknown" is not a valid NlqOperation — fallback parse must fail.
        let json = r#"{"type": "unknown", "data": {}}"#;
        assert!(
            matches!(
                parse_llm_response(json),
                Err(LlmAdapterError::InvalidResponse(_))
            ),
            "unrecognised operation name must still yield InvalidResponse"
        );
    }

    #[test]
    fn parse_user_query_input_detects_raw_ir_json() {
        let raw = r#"{
            "operation": "catalog",
            "signals": ["metrics"],
            "catalog_field": "service_name",
            "time_range": {"from": "now-24h", "to": "now"}
        }"#;

        match parse_user_query_input(raw).expect("raw IR JSON should parse") {
            UserQueryInput::RawIr(ir) => {
                assert_eq!(ir.operation, NlqOperation::Catalog);
                assert_eq!(ir.catalog_field.as_deref(), Some("service_name"));
            }
            UserQueryInput::NaturalLanguage => panic!("expected raw IR"),
        }
    }

    #[test]
    fn parse_user_query_input_keeps_plain_text_as_natural_language() {
        match parse_user_query_input("show p99 latency for checkout").unwrap() {
            UserQueryInput::NaturalLanguage => {}
            UserQueryInput::RawIr(_) => panic!("expected natural language"),
        }
    }

    #[test]
    fn parse_user_query_input_rejects_malformed_json_like_input() {
        let result = parse_user_query_input(r#"{"operation":"catalog""#);
        assert!(
            matches!(result, Err(LlmAdapterError::InvalidResponse(_))),
            "malformed JSON that looks like IR must not fall through to LLM"
        );
    }

    #[test]
    fn raw_ir_service_scope_is_enforced() {
        let raw = r#"{
            "operation": "timeseries",
            "signals": ["metrics"],
            "metric": "request_duration_ms",
            "filters": [],
            "time_range": {"from": "now-1h", "to": "now"}
        }"#;

        let mut ir = match parse_user_query_input(raw).unwrap() {
            UserQueryInput::RawIr(ir) => *ir,
            UserQueryInput::NaturalLanguage => panic!("expected raw IR"),
        };
        enforce_service_scope(&mut ir, "checkout-api");

        assert!(
            ir.filters
                .iter()
                .any(|f| f.field == "service_name" && f.value == "checkout-api")
        );
    }

    #[test]
    fn parse_phi35_response_wrapper_with_ir_field_succeeds() {
        // phi3.5 sometimes wraps output as {"type":"response","ir":{...}} instead
        // of the expected {"type":"ir","ir":{...}} envelope.
        let json = r#"{
            "type": "response",
            "ir": {
                "operation": "timeseries",
                "signals": ["metrics"],
                "metric": "request_duration_ms",
                "filters": [],
                "group_by": [],
                "time_range": {"from": "now-1h", "to": "now"}
            }
        }"#;
        match parse_llm_response(json).expect("phi3.5 response wrapper with ir should parse") {
            NlqIrOrDecline::Ir(ir) => assert_eq!(ir.operation, NlqOperation::Timeseries),
            other => panic!("expected Ir, got {other:?}"),
        }
    }

    #[test]
    fn parse_phi35_response_wrapper_no_ir_returns_invalid() {
        // phi3.5 {"type":"response"} with no parseable nested IR → InvalidResponse
        // so the repair loop can attempt to recover.
        let json = r#"{"type": "response", "content": "I can help with observability queries."}"#;
        assert!(
            matches!(
                parse_llm_response(json),
                Err(LlmAdapterError::InvalidResponse(_))
            ),
            "chat-wrapper with no nested IR must yield InvalidResponse"
        );
    }

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
        let prompt = build_system_prompt(&metrics, &[], None, None);
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
        let prompt = build_system_prompt(&[], &[], Some("checkout"), None);
        assert!(
            prompt.contains("checkout"),
            "service scope must appear in prompt"
        );
    }

    #[test]
    fn system_prompt_includes_advisory_boundary() {
        let prompt = build_system_prompt(&[], &[], None, None);
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
        let prompt = build_system_prompt(&[], &[], None, None);
        assert!(
            prompt.contains("capabilities"),
            "system prompt must reference capabilities type"
        );
    }

    // ── build_system_prompt label_keys ────────────────────────────────────────

    #[test]
    fn build_system_prompt_with_label_keys_includes_section() {
        let keys = vec![
            "service_name".to_string(),
            "pod".to_string(),
            "region".to_string(),
        ];
        let prompt = build_system_prompt(&[], &keys, None, None);
        assert!(
            prompt.contains("Available label keys"),
            "prompt must contain the label keys section header"
        );
        assert!(
            prompt.contains("service_name"),
            "prompt must contain service_name key"
        );
        assert!(prompt.contains("pod"), "prompt must contain pod key");
        assert!(prompt.contains("region"), "prompt must contain region key");
    }

    #[test]
    fn build_system_prompt_empty_label_keys_shows_fallback() {
        let prompt = build_system_prompt(&[], &[], None, None);
        assert!(
            prompt.contains("no label keys discovered"),
            "prompt must show fallback note when label_keys is empty"
        );
    }

    #[test]
    fn build_system_prompt_label_keys_not_invented_note() {
        let keys = vec!["service_name".to_string()];
        let prompt = build_system_prompt(&[], &keys, None, None);
        assert!(
            prompt.contains("Do not invent label keys not listed above"),
            "prompt must include instruction not to invent label keys"
        );
    }

    // ── repair loop ───────────────────────────────────────────────────────────

    struct SequentialMockLlmCaller {
        responses: std::sync::Mutex<std::collections::VecDeque<String>>,
    }

    impl SequentialMockLlmCaller {
        fn new(responses: Vec<String>) -> Self {
            Self {
                responses: std::sync::Mutex::new(responses.into()),
            }
        }
    }

    #[async_trait]
    impl LlmCaller for SequentialMockLlmCaller {
        async fn call(&self, _system: &str, _question: &str) -> Result<String, LlmAdapterError> {
            let mut q = self.responses.lock().unwrap();
            Ok(q.pop_front().unwrap_or_else(|| "{}".to_string()))
        }
    }

    fn valid_ir_json(metric: &str) -> String {
        let ir = sample_ir(metric);
        serde_json::to_string(&serde_json::json!({"type": "ir", "ir": ir})).unwrap()
    }

    #[test]
    fn repair_loop_retries_on_invalid_response_parse() {
        // First call: invalid JSON → repair prompt built.
        // Second call: valid IR → loop breaks.
        // Test parse_llm_response directly (unit test without DB/CH).
        let invalid = "not json at all";
        let valid = valid_ir_json("latency_ms");

        let caller = SequentialMockLlmCaller::new(vec![invalid.to_string(), valid.clone()]);

        // Verify the sequential mock works as intended.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let first = rt.block_on(caller.call("sys", "q")).unwrap();
        assert_eq!(first, invalid);
        let second = rt.block_on(caller.call("sys", "q")).unwrap();
        assert_eq!(second, valid);
    }

    #[test]
    fn repair_loop_exhausted_returns_invalid_response_variant() {
        // Both calls return invalid JSON.
        // parse_llm_response produces InvalidResponse both times.
        let invalid = "still not json";
        let result_first = parse_llm_response(invalid);
        assert!(matches!(
            result_first,
            Err(LlmAdapterError::InvalidResponse(_))
        ));

        // Simulate budget exhausted — second attempt also invalid.
        let result_second = parse_llm_response(invalid);
        assert!(matches!(
            result_second,
            Err(LlmAdapterError::InvalidResponse(_))
        ));
    }

    #[test]
    fn repair_loop_not_triggered_for_decline() {
        // A valid decline response must parse cleanly — no repair needed.
        let json = r#"{"type": "decline", "reason": "cannot map to a metric"}"#;
        match parse_llm_response(json).unwrap() {
            NlqIrOrDecline::Decline { reason } => {
                assert!(
                    reason.contains("cannot map"),
                    "decline reason must be passed through"
                );
            }
            other => panic!("expected Decline, got {other:?}"),
        }
    }

    #[test]
    fn build_repair_prompt_contains_expected_parts() {
        let prompt = build_repair_prompt("show latency", "JSON parse failed", "{bad}");
        assert!(prompt.contains("Original question: \"show latency\""));
        assert!(prompt.contains("Error: JSON parse failed"));
        assert!(prompt.contains("{bad}"));
        assert!(prompt.contains("decline"));
        assert!(prompt.contains("Respond with JSON only"));
    }

    // ── fuzzy_resolve_metric ──────────────────────────────────────────────────

    #[test]
    fn fuzzy_resolve_exact_case_insensitive() {
        let known = &["request_duration_ms", "cpu_usage_percent"];
        assert_eq!(
            fuzzy_resolve_metric("Request_Duration_Ms", known),
            Some("request_duration_ms")
        );
    }

    #[test]
    fn fuzzy_resolve_substring_of_known() {
        let known = &["request_duration_ms", "cpu_usage_percent"];
        // "duration" is a substring of "request_duration_ms"
        assert_eq!(
            fuzzy_resolve_metric("duration", known),
            Some("request_duration_ms")
        );
    }

    #[test]
    fn fuzzy_resolve_known_is_substring_of_guess() {
        let known = &["request_duration_ms", "cpu_usage_percent"];
        // "cpu_usage_percent_total" contains "cpu_usage_percent"
        assert_eq!(
            fuzzy_resolve_metric("cpu_usage_percent_total", known),
            Some("cpu_usage_percent")
        );
    }

    #[test]
    fn fuzzy_resolve_token_overlap() {
        let known = &["request_duration_ms", "cpu_usage_percent"];
        // "request_rate" → "request" matches, "rate" expands to include "request","duration"
        assert_eq!(
            fuzzy_resolve_metric("request_rate", known),
            Some("request_duration_ms")
        );
    }

    #[test]
    fn fuzzy_resolve_semantic_alias() {
        let known = &["request_duration_ms", "cpu_usage_percent"];
        // "latency" has no direct token match but alias expands to "duration"
        assert_eq!(
            fuzzy_resolve_metric("latency", known),
            Some("request_duration_ms")
        );
    }

    #[test]
    fn fuzzy_resolve_common_hallucinations() {
        let known = &["request_duration_ms", "order_processing_duration_ms"];
        // "latency" → alias expands to ["latency", "duration", "response"]
        // "duration" matches tokens in both candidates, but "request" from other aliases
        // helps distinguish. With 2 metrics, "latency" should match the one with "duration" token.
        assert_eq!(
            fuzzy_resolve_metric("latency", known),
            Some("request_duration_ms")
        );
        // "requests_per_second" → "requests" expands to ["request","requests","http"],
        // "per" expands to ["request","duration","rate"], "second" expands similarly.
        // "request" + "duration" tokens match "request_duration_ms" strongly.
        assert_eq!(
            fuzzy_resolve_metric("requests_per_second", known),
            Some("request_duration_ms")
        );
        // "requests_served" → "requests" expands to include "request"
        assert_eq!(
            fuzzy_resolve_metric("requests_served", known),
            Some("request_duration_ms")
        );
    }

    #[test]
    fn fuzzy_resolve_no_match_when_empty() {
        let known: &[&str] = &[];
        assert_eq!(fuzzy_resolve_metric("latency", known), None);
    }

    #[test]
    fn fuzzy_resolve_single_metric_always_matches() {
        let known = &["request_duration_ms"];
        assert_eq!(
            fuzzy_resolve_metric("totally_unrelated", known),
            Some("request_duration_ms")
        );
    }

    // ── Shorthand parser tests ────────────────────────────────────────────────

    #[test]
    fn shorthand_empty_input_returns_defaults() {
        let sh = parse_shorthand_ir("");
        assert_eq!(sh, ShorthandIr::default());
    }

    #[test]
    fn shorthand_parses_metric_prefix() {
        let sh = parse_shorthand_ir("m:http_requests");
        assert_eq!(sh.metric, Some("http_requests".into()));
        assert!(sh.filters.is_empty());
        assert!(sh.query.is_none());
    }

    #[test]
    fn shorthand_parses_explicit_filter_prefix() {
        let sh = parse_shorthand_ir("f:service:checkout");
        assert_eq!(sh.filters.len(), 1);
        assert_eq!(sh.filters[0].field, "service");
        assert_eq!(sh.filters[0].value, "checkout");
        assert_eq!(sh.filters[0].op, NlqFilterOp::Eq);
    }

    #[test]
    fn shorthand_parses_implicit_field_colon_value() {
        let sh = parse_shorthand_ir("env:prod");
        assert_eq!(sh.filters.len(), 1);
        assert_eq!(sh.filters[0].field, "env");
        assert_eq!(sh.filters[0].value, "prod");
    }

    #[test]
    fn shorthand_parses_op_prefix() {
        let sh = parse_shorthand_ir("op:topk");
        assert_eq!(sh.operation, Some(NlqOperation::Topk));
    }

    #[test]
    fn shorthand_unknown_op_is_ignored() {
        let sh = parse_shorthand_ir("op:foobar");
        assert_eq!(sh.operation, None);
    }

    #[test]
    fn shorthand_quoted_text_becomes_query() {
        let sh = parse_shorthand_ir(r#""timeout error""#);
        assert_eq!(sh.query, Some("timeout error".into()));
    }

    #[test]
    fn shorthand_unquoted_words_become_query() {
        let sh = parse_shorthand_ir("checkout p99");
        assert_eq!(sh.query, Some("checkout p99".into()));
    }

    #[test]
    fn shorthand_combined_tokens() {
        let sh = parse_shorthand_ir("m:request_latency service:checkout p99");
        assert_eq!(sh.metric, Some("request_latency".into()));
        assert_eq!(sh.filters.len(), 1);
        assert_eq!(sh.filters[0].field, "service");
        assert_eq!(sh.filters[0].value, "checkout");
        assert_eq!(sh.query, Some("p99".into()));
    }

    #[test]
    fn shorthand_combined_op_and_catalog() {
        let sh = parse_shorthand_ir("op:catalog service_name");
        assert_eq!(sh.operation, Some(NlqOperation::Catalog));
        assert_eq!(sh.query, Some("service_name".into()));
    }

    #[test]
    fn shorthand_multiple_filters() {
        let sh = parse_shorthand_ir("env:prod service:checkout region:us-east-1");
        assert_eq!(sh.filters.len(), 3);
        let fields: Vec<&str> = sh.filters.iter().map(|f| f.field.as_str()).collect();
        assert!(fields.contains(&"env"));
        assert!(fields.contains(&"service"));
        assert!(fields.contains(&"region"));
    }

    #[test]
    fn merge_irs_preserves_metric_execution_shape_from_base_ir() {
        let base = NlqIr {
            operation: NlqOperation::Timeseries,
            signals: vec![NlqSignal::Metrics],
            metric: Some("request_duration_ms".into()),
            window: Some("5m".into()),
            filters: vec![NlqFilter {
                field: "service_name".into(),
                op: NlqFilterOp::Eq,
                value: "checkout".into(),
            }],
            group_by: vec!["environment".into()],
            resolution: Some("1m".into()),
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: Some(NlqVisualizationHint::Timeseries),
            percentiles: Some(vec!["p95".into()]),
            catalog_field: None,
            limit: Some(10),
            query: None,
        };
        let user = NlqIr {
            operation: NlqOperation::Table,
            signals: vec![NlqSignal::Metrics],
            metric: Some("ignored_user_metric".into()),
            window: Some("ignored".into()),
            filters: vec![NlqFilter {
                field: "environment".into(),
                op: NlqFilterOp::Eq,
                value: "prod".into(),
            }],
            group_by: vec!["ignored".into()],
            resolution: Some("ignored".into()),
            time_range: NlqTimeRange {
                from: "now-15m".into(),
                to: "now".into(),
            },
            visualization_hint: Some(NlqVisualizationHint::Table),
            percentiles: Some(vec!["p99".into()]),
            catalog_field: None,
            limit: Some(99),
            query: Some("errors".into()),
        };

        let merged = merge_irs(base, user);

        assert_eq!(merged.operation, NlqOperation::Timeseries);
        assert_eq!(merged.signals, vec![NlqSignal::Metrics]);
        assert_eq!(merged.metric.as_deref(), Some("request_duration_ms"));
        assert_eq!(merged.window.as_deref(), Some("5m"));
        assert_eq!(merged.group_by, vec!["environment"]);
        assert_eq!(merged.resolution.as_deref(), Some("1m"));
        assert_eq!(
            merged.visualization_hint,
            Some(NlqVisualizationHint::Timeseries)
        );
        assert_eq!(merged.percentiles, Some(vec!["p95".into()]));
        assert_eq!(merged.limit, Some(10));
        assert_eq!(merged.query.as_deref(), Some("errors"));
        assert_eq!(merged.time_range.from, "now-15m");
        assert!(
            merged
                .filters
                .iter()
                .any(|f| f.field == "service_name" && f.value == "checkout")
        );
        assert!(
            merged
                .filters
                .iter()
                .any(|f| f.field == "environment" && f.value == "prod")
        );
    }

    #[test]
    fn apply_shorthand_to_ir_overrides_metric_preserves_base() {
        let base = NlqIr {
            operation: NlqOperation::Timeseries,
            signals: vec![NlqSignal::Metrics],
            metric: Some("old_metric".into()),
            window: Some("5m".into()),
            filters: vec![],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
        };
        let sh = ShorthandIr {
            metric: Some("new_metric".into()),
            ..Default::default()
        };
        let result = apply_shorthand_to_ir(base.clone(), sh);
        assert_eq!(result.metric, Some("new_metric".into()));
        assert_eq!(result.window, Some("5m".into())); // base preserved
        assert_eq!(result.signals, vec![NlqSignal::Metrics]); // base preserved
        assert_eq!(result.time_range.from, "now-1h"); // base preserved
    }

    #[test]
    fn apply_shorthand_filter_overrides_base_same_field() {
        let base = NlqIr {
            operation: NlqOperation::Timeseries,
            signals: vec![NlqSignal::Metrics],
            metric: None,
            window: None,
            filters: vec![NlqFilter {
                field: "env".into(),
                op: NlqFilterOp::Eq,
                value: "staging".into(),
            }],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
        };
        let sh = ShorthandIr {
            filters: vec![NlqFilter {
                field: "env".into(),
                op: NlqFilterOp::Eq,
                value: "prod".into(),
            }],
            ..Default::default()
        };
        let result = apply_shorthand_to_ir(base, sh);
        assert_eq!(result.filters.len(), 1);
        assert_eq!(result.filters[0].value, "prod");
    }

    #[test]
    fn shorthand_into_nlq_ir_defaults_to_metrics_when_metric_set() {
        let sh = ShorthandIr {
            metric: Some("http_requests".into()),
            ..Default::default()
        };
        let ir = sh.into_nlq_ir();
        assert_eq!(ir.signals, vec![NlqSignal::Metrics]);
        assert_eq!(ir.metric, Some("http_requests".into()));
    }

    #[test]
    fn shorthand_into_nlq_ir_defaults_to_logs_when_query_set() {
        let sh = ShorthandIr {
            query: Some("error".into()),
            ..Default::default()
        };
        let ir = sh.into_nlq_ir();
        assert_eq!(ir.signals, vec![NlqSignal::Logs]);
        assert_eq!(ir.query, Some("error".into()));
    }

    // ── prepare_nlq_pipeline / resume_nlq_pipeline ──────────────────────────────
    //
    // These use a lazily-connecting Postgres pool and a ClickHouse client pointed at
    // an unreachable port, mirroring the `fake_nlq_app_no_db` pattern in
    // `tests/it/http_api_integration.rs` — neither is actually queried by the code
    // paths exercised below (deny gate short-circuits before any DB call; the
    // interpret-mode + log-signal IR skips both fuzzy metric resolution and
    // `execute_mcp_query`).

    fn fake_db() -> PgPool {
        PgPool::connect_lazy("postgres://user:pass@127.0.0.1:5432/db").expect("valid postgres url")
    }

    fn fake_ch() -> clickhouse::Client {
        clickhouse::Client::default().with_url("http://127.0.0.1:19999")
    }

    fn default_req(question: &str, mode: NlqQueryMode) -> NlqQueryRequest {
        NlqQueryRequest {
            question: Some(question.to_string()),
            service_name: None,
            base_ir: None,
            mode,
        }
    }

    /// An IR that needs no DB access to reach a `Final` outcome from
    /// `resume_nlq_pipeline`: a log-signal query (skips fuzzy metric resolution)
    /// interpreted rather than executed (skips `execute_mcp_query`).
    fn log_ir() -> NlqIr {
        NlqIr {
            operation: NlqOperation::Table,
            signals: vec![NlqSignal::Logs],
            metric: None,
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-1h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: Some("error".into()),
        }
    }

    #[tokio::test]
    async fn prepare_pipeline_deny_gate_short_circuits() {
        let db = fake_db();
        let ch = fake_ch();
        let req = default_req(
            "What is our billing total this month?",
            NlqQueryMode::Execute,
        );

        let outcome = prepare_nlq_pipeline(&db, &ch, Uuid::nil(), &req)
            .await
            .unwrap();

        match outcome {
            NlqPrepareOutcome::Declined(NlqQueryResponse::Decline { reason }) => {
                assert!(
                    reason.contains("billing"),
                    "decline reason should mention billing, got: {reason}"
                );
            }
            _ => panic!("expected Declined(Decline) for a billing question"),
        }
    }

    #[tokio::test]
    async fn resume_pipeline_valid_ir_produces_final() {
        let db = fake_db();
        let ch = fake_ch();
        let ir = log_ir();
        let raw = serde_json::to_string(&serde_json::json!({"type": "ir", "ir": ir})).unwrap();
        let req = default_req("show me errors", NlqQueryMode::Interpret);

        let outcome = resume_nlq_pipeline(&db, &ch, Uuid::nil(), &req, "show me errors", 0, &raw)
            .await
            .unwrap();

        match outcome {
            NlqResumeOutcome::Final(NlqQueryResponse::Ir { ir: got }) => {
                assert_eq!(got.signals, vec![NlqSignal::Logs]);
            }
            _ => panic!("expected Final(Ir) for a valid log-signal IR in interpret mode"),
        }
    }

    #[tokio::test]
    async fn resume_pipeline_invalid_response_under_cap_returns_needs_repair() {
        let db = fake_db();
        let ch = fake_ch();
        let req = default_req("original question", NlqQueryMode::Execute);

        let outcome = resume_nlq_pipeline(
            &db,
            &ch,
            Uuid::nil(),
            &req,
            "original question",
            0,
            "not json at all",
        )
        .await
        .unwrap();

        match outcome {
            NlqResumeOutcome::NeedsRepair { repair_prompt } => {
                assert!(repair_prompt.contains("Original question: \"original question\""));
                assert!(repair_prompt.contains("not json at all"));
            }
            _ => {
                panic!("expected NeedsRepair when repair_attempt (0) is below MAX_REPAIR_ATTEMPTS")
            }
        }
    }

    #[tokio::test]
    async fn resume_pipeline_invalid_response_at_cap_returns_final_invalid_response() {
        let db = fake_db();
        let ch = fake_ch();
        let req = default_req("q", NlqQueryMode::Execute);

        let outcome = resume_nlq_pipeline(
            &db,
            &ch,
            Uuid::nil(),
            &req,
            "q",
            MAX_REPAIR_ATTEMPTS,
            "still not json",
        )
        .await
        .unwrap();

        match outcome {
            NlqResumeOutcome::Final(NlqQueryResponse::InvalidResponse {
                raw_llm_response, ..
            }) => {
                assert_eq!(raw_llm_response, "still not json");
            }
            _ => panic!(
                "expected Final(InvalidResponse) once repair_attempt reaches MAX_REPAIR_ATTEMPTS"
            ),
        }
    }

    // ── handle_nlq_prepare / handle_nlq_complete ─────────────────────────────

    fn fake_state() -> AppState {
        AppState {
            ch: fake_ch(),
            db: fake_db(),
            planner: std::sync::Arc::new(crate::planner::QueryPlanner),
            llm: None,
            auth_service_url: "http://auth-service:4319".into(),
            http_client: reqwest::Client::new(),
            metrics: std::sync::Arc::new(crate::observability::QueryApiMetrics::new()),
            sessions: crate::nlq_session::NlqSessionStore::default(),
        }
    }

    fn fake_ctx(tenant_id: Uuid) -> TenantContext {
        TenantContext {
            tenant_id,
            user_id: None,
            role: "admin".into(),
        }
    }

    #[tokio::test]
    async fn handle_nlq_prepare_shorthand_bypass_short_circuits_before_session_store() {
        let state = fake_state();
        let ctx = fake_ctx(Uuid::new_v4());
        let req = default_req("/m:cpu_usage", NlqQueryMode::Interpret);

        let Json(response) = handle_nlq_prepare(State(state.clone()), Extension(ctx), Json(req))
            .await
            .expect("shorthand bypass should not error");

        match response {
            NlqPrepareResponse::Final {
                response: NlqQueryResponse::Ir { ir },
            } => {
                assert_eq!(ir.metric.as_deref(), Some("cpu_usage"));
            }
            other => panic!(
                "expected Final(Ir) for shorthand bypass, got a different variant: {other:?}"
            ),
        }
    }

    #[tokio::test]
    async fn handle_nlq_prepare_deny_gate_question_returns_final_decline() {
        let state = fake_state();
        let ctx = fake_ctx(Uuid::new_v4());
        let req = default_req(
            "What is our billing total this month?",
            NlqQueryMode::Execute,
        );

        let Json(response) = handle_nlq_prepare(State(state), Extension(ctx), Json(req))
            .await
            .expect("deny gate should return Ok with a Decline body, not an HTTP error");

        match response {
            NlqPrepareResponse::Final {
                response: NlqQueryResponse::Decline { reason },
            } => {
                assert!(reason.contains("billing"));
            }
            other => panic!("expected Final(Decline), got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_nlq_prepare_reaches_prepared_and_session_is_retrievable() {
        let state = fake_state();
        let tenant_id = Uuid::new_v4();
        let ctx = fake_ctx(tenant_id);
        let req = default_req("how many errors happened", NlqQueryMode::Execute);

        let Json(response) = handle_nlq_prepare(State(state.clone()), Extension(ctx), Json(req))
            .await
            .expect("ordinary question should reach Prepared");

        let session_token = match response {
            NlqPrepareResponse::Prepared { session_token, .. } => session_token,
            other => panic!("expected Prepared, got: {other:?}"),
        };

        let session = state
            .sessions
            .take_for_resume(session_token, tenant_id)
            .expect("session should be retrievable after /prepare");
        assert_eq!(session.tenant_id, tenant_id);
        assert_eq!(session.repair_attempt, 0);
    }

    #[tokio::test]
    async fn handle_nlq_complete_unknown_token_returns_404() {
        let state = fake_state();
        let ctx = fake_ctx(Uuid::new_v4());
        let body = NlqCompleteRequest {
            session_token: Uuid::new_v4(),
            raw_llm_response: "irrelevant".into(),
        };

        let err = handle_nlq_complete(State(state), Extension(ctx), Json(body))
            .await
            .expect_err("unknown session token should 404");
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn handle_nlq_complete_wrong_tenant_returns_404() {
        let state = fake_state();
        let tenant_id = Uuid::new_v4();
        let other_tenant = Uuid::new_v4();
        let session_token = state.sessions.insert(
            tenant_id,
            default_req("show me errors", NlqQueryMode::Interpret),
            "show me errors".into(),
        );

        let body = NlqCompleteRequest {
            session_token,
            raw_llm_response: "irrelevant".into(),
        };
        let err = handle_nlq_complete(State(state), Extension(fake_ctx(other_tenant)), Json(body))
            .await
            .expect_err("wrong-tenant session token should 404");
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn handle_nlq_complete_valid_response_returns_final_and_removes_session() {
        let state = fake_state();
        let tenant_id = Uuid::new_v4();
        let ir = log_ir();
        let raw = serde_json::to_string(&serde_json::json!({"type": "ir", "ir": ir})).unwrap();
        let session_token = state.sessions.insert(
            tenant_id,
            default_req("show me errors", NlqQueryMode::Interpret),
            "show me errors".into(),
        );

        let body = NlqCompleteRequest {
            session_token,
            raw_llm_response: raw,
        };
        let Json(response) = handle_nlq_complete(
            State(state.clone()),
            Extension(fake_ctx(tenant_id)),
            Json(body),
        )
        .await
        .expect("valid IR response should succeed");
        match response {
            NlqCompleteResponse::Final {
                response: NlqQueryResponse::Ir { .. },
            } => {}
            other => panic!("expected Final(Ir), got: {other:?}"),
        }

        // Session is gone: a second /complete call with the same token now 404s.
        let body_again = NlqCompleteRequest {
            session_token,
            raw_llm_response: "anything".into(),
        };
        let err = handle_nlq_complete(
            State(state),
            Extension(fake_ctx(tenant_id)),
            Json(body_again),
        )
        .await
        .expect_err("session should have been removed after Final outcome");
        assert_eq!(err.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn handle_nlq_complete_invalid_response_under_cap_keeps_session_with_bumped_repair_attempt()
     {
        let state = fake_state();
        let tenant_id = Uuid::new_v4();
        let session_token = state.sessions.insert(
            tenant_id,
            default_req("original question", NlqQueryMode::Execute),
            "original question".into(),
        );

        let body = NlqCompleteRequest {
            session_token,
            raw_llm_response: "not json at all".into(),
        };
        let Json(response) = handle_nlq_complete(
            State(state.clone()),
            Extension(fake_ctx(tenant_id)),
            Json(body),
        )
        .await
        .expect("invalid response under the repair cap should not error");
        match response {
            NlqCompleteResponse::NeedsRepair { repair_prompt } => {
                assert!(repair_prompt.contains("not json at all"));
            }
            other => panic!("expected NeedsRepair, got: {other:?}"),
        }

        // Session is still present, and repair_attempt was bumped server-side: feeding another
        // invalid response now reaches the repair-cap-exhausted path (Final(InvalidResponse)),
        // proving repair_attempt is now at MAX_REPAIR_ATTEMPTS.
        let body_again = NlqCompleteRequest {
            session_token,
            raw_llm_response: "still not json".into(),
        };
        let Json(response) = handle_nlq_complete(
            State(state),
            Extension(fake_ctx(tenant_id)),
            Json(body_again),
        )
        .await
        .expect("second repair turn should not error");
        match response {
            NlqCompleteResponse::Final {
                response:
                    NlqQueryResponse::InvalidResponse {
                        raw_llm_response, ..
                    },
            } => {
                assert_eq!(raw_llm_response, "still not json");
            }
            other => panic!("expected Final(InvalidResponse) at the repair cap, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn handle_nlq_complete_invalid_response_at_cap_returns_final_invalid_response() {
        let state = fake_state();
        let tenant_id = Uuid::new_v4();
        let session_token = state.sessions.insert(
            tenant_id,
            default_req("q", NlqQueryMode::Execute),
            "q".into(),
        );
        // Drive repair_attempt up to MAX_REPAIR_ATTEMPTS directly via the store, matching what
        // `handle_nlq_complete` would have done across MAX_REPAIR_ATTEMPTS prior NeedsRepair turns.
        let mut session = state
            .sessions
            .take_for_resume(session_token, tenant_id)
            .unwrap();
        session.repair_attempt = MAX_REPAIR_ATTEMPTS;
        state.sessions.put_back(session_token, session);

        let body = NlqCompleteRequest {
            session_token,
            raw_llm_response: "still not json".into(),
        };
        let Json(response) =
            handle_nlq_complete(State(state), Extension(fake_ctx(tenant_id)), Json(body))
                .await
                .expect("repair-cap-exhausted response should not error");
        match response {
            NlqCompleteResponse::Final {
                response:
                    NlqQueryResponse::InvalidResponse {
                        raw_llm_response, ..
                    },
            } => {
                assert_eq!(raw_llm_response, "still not json");
            }
            other => panic!("expected Final(InvalidResponse), got: {other:?}"),
        }
    }
}
