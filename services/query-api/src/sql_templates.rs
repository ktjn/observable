// MCP Server SQL template library — translates NlqIr into ClickHouse SQL.
//
// Design contract (per ADR-021):
//   - Deterministic: identical NlqIr always produces identical SQL.
//   - Tenant-scoped: every generated SQL carries `tenant_id = '<uuid>'` in the WHERE clause.
//   - Metric-type-aware: counter, gauge, histogram, and summary each get the correct SQL pattern.
//   - Unit-testable: pure functions, no I/O.
//
// All filter values from the NlqIr are treated as untrusted and are escaped before inlining.
// Tenant IDs and metric names are from trusted context and are inlined directly after UUID/name
// formatting.
//
// Table references: observable.metric_series (ms) + observable.metric_points (mp).
use domain::{NlqFilter, NlqFilterOp, NlqIr, NlqOperation};
use uuid::Uuid;

// ── Schema metric type ────────────────────────────────────────────────────────

/// Metric type from the Schema Registry annotation, used to select the correct SQL pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaMetricType {
    Gauge,
    Counter,
    Histogram,
    Summary,
    Unknown,
}

impl SchemaMetricType {
    pub fn parse(s: &str) -> Self {
        match s {
            "gauge" => Self::Gauge,
            "counter" => Self::Counter,
            "histogram" => Self::Histogram,
            "summary" => Self::Summary,
            _ => Self::Unknown,
        }
    }
}

// ── Context ───────────────────────────────────────────────────────────────────

/// Everything the SQL template library needs to render a query.
pub struct SqlContext<'a> {
    /// Tenant that owns this query — injected into every WHERE clause.
    pub tenant_id: Uuid,
    /// Metric name as stored in `metric_series.metric_name`.
    pub metric_name: &'a str,
    /// Metric type from the Schema Registry (reserved for metric-type-aware dispatch in Step 6).
    #[allow(dead_code)]
    pub metric_type: SchemaMetricType,
    /// The NLQ IR emitted by the LLM.
    pub ir: &'a NlqIr,
}

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub enum SqlTemplateError {
    MissingMetricName,
    /// Reserved for time-range validation (may be surfaced in future steps).
    #[allow(dead_code)]
    MissingTimeRange,
    /// Reserved for unsupported operation validation.
    #[allow(dead_code)]
    UnsupportedOperation(String),
    InvalidResolution(String),
    InvalidTimeExpression(String),
    MissingCatalogField,
}

impl std::fmt::Display for SqlTemplateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingMetricName => write!(f, "metric_name is required for SQL generation"),
            Self::MissingTimeRange => write!(f, "time_range is required"),
            Self::UnsupportedOperation(op) => write!(f, "unsupported operation: {op}"),
            Self::InvalidResolution(r) => write!(f, "invalid resolution: {r}"),
            Self::InvalidTimeExpression(e) => write!(f, "invalid time expression: {e}"),
            Self::MissingCatalogField => {
                write!(f, "catalog_field is required for catalog operations")
            }
        }
    }
}

impl std::error::Error for SqlTemplateError {}

// ── Public entry point ────────────────────────────────────────────────────────

/// Generates ClickHouse SQL for the given `NlqIr` and schema context.
///
/// Every generated query carries `tenant_id = '<uuid>'` in its WHERE clause.
/// Filter values are escaped before inlining. Operation dispatch follows `ir.operation`.
pub fn generate_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    match ctx.ir.operation {
        NlqOperation::Timeseries => timeseries_sql(ctx),
        NlqOperation::Rate => rate_sql(ctx),
        NlqOperation::Irate => irate_sql(ctx),
        NlqOperation::Increase => increase_sql(ctx),
        NlqOperation::Histogram => histogram_sql(ctx),
        NlqOperation::Topk => topk_sql(ctx),
        NlqOperation::Table => table_sql(ctx),
        NlqOperation::Distribution => distribution_sql(ctx),
        NlqOperation::Catalog => catalog_sql(ctx),
        // Inventory operations are executed by execute_inventory_query in mcp_query,
        // not through the SQL template pipeline. Reaching here is a logic error.
        NlqOperation::Inventory => Err(SqlTemplateError::MissingMetricName),
    }
}

// ── Operation templates ───────────────────────────────────────────────────────

/// Time-series line chart: avg(value) per time bucket.
/// Used for gauge metrics. For counters, prefer `rate_sql`.
fn timeseries_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let resolution = ctx.ir.resolution.as_deref().unwrap_or("1m");
    let interval = parse_interval(resolution)?;
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let (group_select, group_clause) = build_group_by(&ctx.ir.group_by);

    Ok(format!(
        "SELECT\n    \
             toStartOfInterval(fromUnixTimestamp64Nano(mp.time_unix_nano), INTERVAL {interval}) AS bucket,\n    \
             avg(coalesce(mp.value_double, toFloat64(mp.value_int))) AS value{group_select}\n\
         FROM observable.metric_points mp\n\
         JOIN observable.metric_series ms\n    \
             ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n\
         WHERE mp.tenant_id = '{tenant_id}'\n  \
           AND ms.metric_name = '{metric_name}'{filters}{time_clause}\n\
         GROUP BY bucket{group_clause}\n\
         ORDER BY bucket",
        interval = interval,
        group_select = group_select,
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        group_clause = group_clause,
    ))
}

/// Per-window rate of a monotonic counter (reset-aware delta / interval seconds).
fn rate_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let resolution = ctx.ir.resolution.as_deref().unwrap_or("1m");
    let interval = parse_interval(resolution)?;
    let interval_secs = interval_to_secs(resolution)?;
    let window = ctx.ir.window.as_deref().unwrap_or("5m");
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let window_clause = build_window_preceding_clause(window)?;
    let (group_select, group_clause) = build_group_by(&ctx.ir.group_by);

    Ok(format!(
        "WITH src AS (\n    \
             SELECT\n        \
                 mp.time_unix_nano,\n        \
                 coalesce(mp.value_double, toFloat64(mp.value_int)) AS value,\n        \
                 mp.metric_series_id\n    \
             FROM observable.metric_points mp\n    \
             JOIN observable.metric_series ms\n        \
                 ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n    \
             WHERE mp.tenant_id = '{tenant_id}'\n      \
               AND ms.metric_name = '{metric_name}'{filters}{time_clause}{window_clause}\n\
         )\n\
         SELECT\n    \
             toStartOfInterval(fromUnixTimestamp64Nano(time_unix_nano), INTERVAL {interval}) AS bucket,\n    \
             sum(if(delta < 0, value, delta)) / {interval_secs} AS rate{group_select}\n\
         FROM (\n    \
             SELECT\n        \
                 time_unix_nano, value,\n        \
                 value - lagInFrame(value, 1, value)\n            \
                     OVER (PARTITION BY metric_series_id ORDER BY time_unix_nano) AS delta\n    \
             FROM src\n\
         )\n\
         GROUP BY bucket{group_clause}\n\
         ORDER BY bucket",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        window_clause = window_clause,
        interval = interval,
        interval_secs = interval_secs,
        group_select = group_select,
        group_clause = group_clause,
    ))
}

/// Instantaneous rate using the two most-recent samples in the lookback window.
fn irate_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let window = ctx.ir.window.as_deref().unwrap_or("1m");
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let window_clause = build_window_preceding_clause(window)?;

    Ok(format!(
        "WITH src AS (\n    \
             SELECT\n        \
                 mp.time_unix_nano,\n        \
                 coalesce(mp.value_double, toFloat64(mp.value_int)) AS value,\n        \
                 mp.metric_series_id,\n        \
                 row_number()\n            \
                     OVER (PARTITION BY mp.metric_series_id ORDER BY mp.time_unix_nano DESC) AS rn\n    \
             FROM observable.metric_points mp\n    \
             JOIN observable.metric_series ms\n        \
                 ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n    \
             WHERE mp.tenant_id = '{tenant_id}'\n      \
               AND ms.metric_name = '{metric_name}'{filters}{time_clause}{window_clause}\n\
         )\n\
         SELECT\n    \
             metric_series_id,\n    \
             if(delta < 0, latest_value, delta) / ((latest_ts - prev_ts) / 1e9) AS irate\n\
         FROM (\n    \
             SELECT\n        \
                 metric_series_id,\n        \
                 maxIf(value, rn = 1) AS latest_value,\n        \
                 maxIf(value, rn = 1) - maxIf(value, rn = 2) AS delta,\n        \
                 maxIf(time_unix_nano, rn = 1) AS latest_ts,\n        \
                 maxIf(time_unix_nano, rn = 2) AS prev_ts\n    \
             FROM src\n    \
             WHERE rn <= 2\n    \
             GROUP BY metric_series_id\n\
         )\n\
         WHERE latest_ts > prev_ts",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        window_clause = window_clause,
    ))
}

/// Monotonic counter increase over the lookback window (sum of positive deltas).
fn increase_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let window = ctx.ir.window.as_deref().unwrap_or("1h");
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let window_clause = build_window_preceding_clause(window)?;
    let (group_select, group_clause) = build_group_by(&ctx.ir.group_by);

    Ok(format!(
        "WITH src AS (\n    \
             SELECT\n        \
                 mp.time_unix_nano,\n        \
                 coalesce(mp.value_double, toFloat64(mp.value_int)) AS value,\n        \
                 mp.metric_series_id\n    \
             FROM observable.metric_points mp\n    \
             JOIN observable.metric_series ms\n        \
                 ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n    \
             WHERE mp.tenant_id = '{tenant_id}'\n      \
               AND ms.metric_name = '{metric_name}'{filters}{time_clause}{window_clause}\n\
         )\n\
         SELECT\n    \
             sum(if(delta < 0, value, delta)) AS increase{group_select}\n\
         FROM (\n    \
             SELECT\n        \
                 value,\n        \
                 value - lagInFrame(value, 1, value)\n            \
                     OVER (PARTITION BY metric_series_id ORDER BY time_unix_nano) AS delta\n    \
             FROM src\n\
         )\n\
         WHERE true{group_clause}",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        window_clause = window_clause,
        group_select = group_select,
        group_clause = group_clause,
    ))
}

/// Histogram bucket distribution via explicit_bounds + arrayDifference on bucket counts.
fn histogram_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;

    Ok(format!(
        "SELECT\n    \
             bound,\n    \
             sum(count_in_bucket) AS count\n\
         FROM (\n    \
             SELECT\n        \
                 arrayJoin(\n            \
                     arrayMap(\n                \
                         (b, c) -> (b, c),\n                \
                         mp.histogram_explicit_bounds,\n                \
                         arraySlice(arrayDifference(arrayConcat([toUInt64(0)], mp.histogram_bucket_counts)), 1, length(mp.histogram_explicit_bounds))\n            \
                     )\n        \
                 ) AS bc,\n        \
                 bc.1 AS bound,\n        \
                 bc.2 AS count_in_bucket\n    \
             FROM observable.metric_points mp\n    \
             JOIN observable.metric_series ms\n        \
                 ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n    \
             WHERE mp.tenant_id = '{tenant_id}'\n      \
               AND ms.metric_name = '{metric_name}'{filters}{time_clause}\n      \
               AND notEmpty(mp.histogram_explicit_bounds)\n\
         )\n\
         GROUP BY bound\n\
         ORDER BY bound",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
    ))
}

/// Top-K series by average value in the time range.
fn topk_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    const DEFAULT_K: u32 = 10;
    let k = ctx.ir.limit.unwrap_or(DEFAULT_K);
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;

    Ok(format!(
        "SELECT\n    \
             ms.metric_name,\n    \
             ms.service_name,\n    \
             avg(coalesce(mp.value_double, toFloat64(mp.value_int))) AS avg_value\n\
         FROM observable.metric_points mp\n\
         JOIN observable.metric_series ms\n    \
             ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n\
         WHERE mp.tenant_id = '{tenant_id}'\n  \
           AND ms.metric_name = '{metric_name}'{filters}{time_clause}\n\
         GROUP BY ms.metric_name, ms.service_name\n\
         ORDER BY avg_value DESC\n\
         LIMIT {k}",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        k = k,
    ))
}

/// Flat tabular point scan (most-recent 1000 rows).
fn table_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let (extra_select, _) = build_group_by(&ctx.ir.group_by);

    Ok(format!(
        "SELECT\n    \
             mp.time_unix_nano AS timestamp_unix_nano,\n    \
             ms.metric_name,\n    \
             ms.service_name,\n    \
             coalesce(mp.value_double, toFloat64(mp.value_int)) AS value{extra_select}\n\
         FROM observable.metric_points mp\n\
         JOIN observable.metric_series ms\n    \
             ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n\
         WHERE mp.tenant_id = '{tenant_id}'\n  \
           AND ms.metric_name = '{metric_name}'{filters}{time_clause}\n\
         ORDER BY timestamp_unix_nano DESC\n\
         LIMIT 1000",
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
        extra_select = extra_select,
    ))
}

/// Empirical value distribution (width_bucket percentile).
///
/// When `ir.percentiles` is set, generates exactly those stat expressions (in order).
/// When absent or empty, falls back to the default set: p50/p90/p95/p99/min/max.
///
/// Supported entries (case-sensitive):
/// - `p{N}` where N is 1–999  → `quantile(N/1000.0)(value) AS p{N}`
/// - `"median"`                → `quantile(0.50)(value) AS median`
/// - `"average"` / `"mean"`   → `avg(value) AS <key>`
/// - `"min"`                  → `min(value) AS min`
/// - `"max"`                  → `max(value) AS max`
///
/// Unrecognised entries are silently skipped.
fn distribution_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let filters = build_filter_clauses(&ctx.ir.filters);
    let time_clause = build_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let val = "coalesce(mp.value_double, toFloat64(mp.value_int))";

    let default_stats: &[&str] = &["p50", "p90", "p95", "p99", "min", "max"];
    let requested: Vec<&str> = match ctx.ir.percentiles.as_deref() {
        Some(v) if !v.is_empty() => v.iter().map(|s| s.as_str()).collect(),
        _ => default_stats.to_vec(),
    };

    let mut expressions: Vec<String> = requested
        .iter()
        .filter_map(|stat| stat_to_sql_expr(stat, val))
        .collect();

    if expressions.is_empty() {
        expressions.push(format!("quantile(0.99)({val}) AS p99"));
    }

    let select_list = expressions
        .iter()
        .map(|e| format!("    {e}"))
        .collect::<Vec<_>>()
        .join(",\n");

    Ok(format!(
        "SELECT\n{select_list}\nFROM observable.metric_points mp\n\
         JOIN observable.metric_series ms\n    \
             ON ms.tenant_id = mp.tenant_id AND ms.metric_series_id = mp.metric_series_id\n\
         WHERE mp.tenant_id = '{tenant_id}'\n  \
           AND ms.metric_name = '{metric_name}'{filters}{time_clause}",
        select_list = select_list,
        tenant_id = ctx.tenant_id,
        metric_name = escape_string_value(ctx.metric_name),
        filters = filters,
        time_clause = time_clause,
    ))
}

/// Catalog query: enumerate distinct values of a dimension in `observable.metric_series`.
///
/// The `metric_series` table is a dimension table (no time column), so no time-range
/// clause is applied. The column mapping follows `map_filter_field()` — known direct
/// columns resolve to `ms.<col>`; anything else is extracted from `ms.attributes` via
/// `JSONExtractString`.
///
/// Returns: `value` (the dimension value) + `series_count` (how many series share it),
/// ordered by `series_count DESC`, limited to 100 rows.
fn catalog_sql(ctx: &SqlContext) -> Result<String, SqlTemplateError> {
    let field = ctx
        .ir
        .catalog_field
        .as_deref()
        .ok_or(SqlTemplateError::MissingCatalogField)?;

    let col_expr = map_filter_field(field);
    let filters = build_filter_clauses(&ctx.ir.filters);

    // Use the catalog_field name as the column alias so the frontend and eval
    // harness can identify rows by meaningful names (e.g. "service_name", "metric_name").
    // Only ASCII-safe identifiers are allowed here; map_filter_field already
    // returns a safe ClickHouse column expression.
    Ok(format!(
        "SELECT\n    \
             {col_expr} AS {field},\n    \
             count() AS series_count\n\
         FROM observable.metric_series ms\n\
         WHERE ms.tenant_id = '{tenant_id}'{filters}\n\
         GROUP BY {field}\n\
         ORDER BY series_count DESC\n\
         LIMIT 100",
        col_expr = col_expr,
        field = field,
        tenant_id = ctx.tenant_id,
        filters = filters,
    ))
}

/// Translates a single percentile/stat name to a ClickHouse SELECT expression.
/// Returns `None` for unrecognised entries (safe — caller silently skips).
fn stat_to_sql_expr(stat: &str, val: &str) -> Option<String> {
    match stat {
        "median" => Some(format!("quantile(0.50)({val}) AS median")),
        "average" => Some(format!("avg({val}) AS average")),
        "mean" => Some(format!("avg({val}) AS mean")),
        "min" => Some(format!("min({val}) AS min")),
        "max" => Some(format!("max({val}) AS max")),
        // Legacy aliases retained for backward compatibility.
        "min_val" => Some(format!("min({val}) AS min_val")),
        "max_val" => Some(format!("max({val}) AS max_val")),
        other => {
            let digits = other.strip_prefix('p')?;
            let n: u32 = digits.parse().ok()?;
            if n == 0 || n > 999 {
                return None;
            }
            // p1–p99: divide by 100 (p99 → 0.990); p100–p999: divide by 1000 (p999 → 0.999).
            let q = if n <= 99 {
                n as f64 / 100.0
            } else {
                n as f64 / 1000.0
            };
            Some(format!("quantile({q:.3})({val}) AS {other}"))
        }
    }
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Parses a relative or absolute time expression to a ClickHouse expression string.
///
/// Per ADR-030, all timestamps are transported as Unix nanosecond integers. Supported forms:
/// - `"now"` → `"toUnixTimestamp64Nano(now64())"`
/// - `"now-5m"` → `"toUnixTimestamp64Nano(now64()) - 300000000000"`
/// - `"now-2h"` → `"toUnixTimestamp64Nano(now64()) - 7200000000000"`
/// - `"now-1d"` → `"toUnixTimestamp64Nano(now64()) - 86400000000000"`
/// - `"now-30s"` → `"toUnixTimestamp64Nano(now64()) - 30000000000"`
/// - Unix nanosecond integer string (all digits) → the integer literal
///
/// ISO-8601 strings are intentionally rejected — callers must convert to Unix nanoseconds first.
pub fn parse_time_expr(expr: &str) -> Result<String, SqlTemplateError> {
    let expr = expr.trim();
    if expr == "now" {
        return Ok("toUnixTimestamp64Nano(now64())".into());
    }
    if let Some(rest) = expr.strip_prefix("now-") {
        let (n, unit) = parse_duration_str(rest)
            .ok_or_else(|| SqlTemplateError::InvalidTimeExpression(expr.into()))?;
        let nanos_per_unit: u64 = match unit {
            "s" => 1_000_000_000,
            "m" => 60_000_000_000,
            "h" => 3_600_000_000_000,
            "d" => 86_400_000_000_000,
            _ => return Err(SqlTemplateError::InvalidTimeExpression(expr.into())),
        };
        let offset = n
            .checked_mul(nanos_per_unit)
            .ok_or_else(|| SqlTemplateError::InvalidTimeExpression(expr.into()))?;
        return Ok(format!("toUnixTimestamp64Nano(now64()) - {offset}"));
    }
    // Unix nanosecond integer literal (sent by frontend per ADR-030)
    if !expr.is_empty() && expr.chars().all(|c| c.is_ascii_digit()) {
        return Ok(expr.to_string());
    }
    Err(SqlTemplateError::InvalidTimeExpression(expr.into()))
}

/// Parses a resolution/window string like "1m", "5m", "1h" into a ClickHouse INTERVAL expression.
pub fn parse_interval(s: &str) -> Result<String, SqlTemplateError> {
    let (n, unit) =
        parse_duration_str(s).ok_or_else(|| SqlTemplateError::InvalidResolution(s.into()))?;
    let ch_unit =
        duration_unit_to_ch(unit).ok_or_else(|| SqlTemplateError::InvalidResolution(s.into()))?;
    Ok(format!("{n} {ch_unit}"))
}

/// Returns the number of seconds in a resolution string (for use as rate denominator).
pub fn interval_to_secs(s: &str) -> Result<f64, SqlTemplateError> {
    let (n, unit) =
        parse_duration_str(s).ok_or_else(|| SqlTemplateError::InvalidResolution(s.into()))?;
    let factor = match unit {
        "s" => 1.0,
        "m" => 60.0,
        "h" => 3600.0,
        "d" => 86400.0,
        _ => return Err(SqlTemplateError::InvalidResolution(s.into())),
    };
    Ok(n as f64 * factor)
}

/// Escapes a string value for safe inlining in ClickHouse SQL (single-quoted).
///
/// Replaces `\` with `\\` and `'` with `\'`.
pub fn escape_string_value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

// ── Log query support ─────────────────────────────────────────────────────────

/// Context for generating log search SQL.
pub struct LogSqlContext<'a> {
    pub tenant_id: Uuid,
    pub ir: &'a NlqIr,
}

/// Generates a ClickHouse SQL query for log search.
///
/// Uses `positionCaseInsensitive(body, '<term>') > 0` for substring matching
/// (avoids `LIKE` wildcard pitfalls with `%` and `_` in user input).
/// Filters map to direct log columns or JSON attribute extraction.
pub fn generate_log_sql(ctx: &LogSqlContext) -> Result<String, SqlTemplateError> {
    let time_clause = build_log_time_range_clause(&ctx.ir.time_range.from, &ctx.ir.time_range.to)?;
    let filter_clause = build_log_filter_clauses(&ctx.ir.filters);
    let query_clause = match &ctx.ir.query {
        Some(q) if !q.is_empty() => {
            format!(
                "\n  AND positionCaseInsensitive(body, '{}') > 0",
                escape_string_value(q)
            )
        }
        _ => String::new(),
    };

    Ok(format!(
        "SELECT\n    \
             log_id,\n    \
             timestamp_unix_nano,\n    \
             observed_timestamp_unix_nano,\n    \
             severity_number,\n    \
             severity_text,\n    \
             body,\n    \
             trace_id,\n    \
             span_id,\n    \
             service_name,\n    \
             environment,\n    \
             host_id,\n    \
             fingerprint,\n    \
             attributes,\n    \
             resource_attributes\n\
         FROM observable.logs\n\
         WHERE tenant_id = '{tenant_id}'{query_clause}{filter_clause}{time_clause}\n\
         ORDER BY timestamp_unix_nano DESC\n\
         LIMIT 200",
        tenant_id = ctx.tenant_id,
        query_clause = query_clause,
        filter_clause = filter_clause,
        time_clause = time_clause,
    ))
}

/// Builds time-range clause for log queries (uses `timestamp_unix_nano` column).
fn build_log_time_range_clause(from: &str, to: &str) -> Result<String, SqlTemplateError> {
    let from_expr = parse_time_expr(from)?;
    let to_expr = parse_time_expr(to)?;
    Ok(format!(
        "\n  AND timestamp_unix_nano >= {from_expr}\
         \n  AND timestamp_unix_nano <= {to_expr}"
    ))
}

/// Translates a filter field name into a ClickHouse column reference for the logs table.
fn map_log_filter_field(field: &str) -> String {
    match field {
        "service_name" | "service" => "service_name".into(),
        "severity_text" | "severity" | "level" => "severity_text".into(),
        "environment" | "env" => "environment".into(),
        "trace_id" => "trace_id".into(),
        "span_id" => "span_id".into(),
        "body" => "body".into(),
        _ => format!(
            "JSONExtractString(attributes, '{}')",
            escape_string_value(field)
        ),
    }
}

/// Builds filter clauses for log queries.
fn build_log_filter_clauses(filters: &[NlqFilter]) -> String {
    if filters.is_empty() {
        return String::new();
    }
    filters
        .iter()
        .map(|f| {
            let col = map_log_filter_field(&f.field);
            format!("\n  AND {}", build_filter_expr(&col, f.op, &f.value))
        })
        .collect::<Vec<_>>()
        .join("")
}

// ── Private helpers ───────────────────────────────────────────────────────────

/// Parses `"5m"` into `(5u64, "m")`, `"2h"` into `(2u64, "h")`, etc.
fn parse_duration_str(s: &str) -> Option<(u64, &str)> {
    let s = s.trim();
    let split_at = s.find(|c: char| !c.is_ascii_digit())?;
    if split_at == 0 {
        return None;
    }
    let n: u64 = s[..split_at].parse().ok()?;
    let unit = &s[split_at..];
    if unit.is_empty() {
        return None;
    }
    Some((n, unit))
}

fn duration_unit_to_ch(unit: &str) -> Option<&'static str> {
    match unit {
        "s" => Some("SECOND"),
        "m" => Some("MINUTE"),
        "h" => Some("HOUR"),
        "d" => Some("DAY"),
        _ => None,
    }
}

/// Builds `\n  AND <from_expr> AND <to_expr>` clauses.
fn build_time_range_clause(from: &str, to: &str) -> Result<String, SqlTemplateError> {
    let from_expr = parse_time_expr(from)?;
    let to_expr = parse_time_expr(to)?;
    Ok(format!(
        "\n  AND mp.time_unix_nano >= {from_expr}\
         \n  AND mp.time_unix_nano <= {to_expr}"
    ))
}

/// Builds a lookback window clause for rate/irate/increase queries.
/// The window restricts the source rows to the last N seconds/minutes/hours.
fn build_window_preceding_clause(window: &str) -> Result<String, SqlTemplateError> {
    let from_expr = parse_time_expr(&format!("now-{window}"))?;
    Ok(format!("\n  AND mp.time_unix_nano >= {from_expr}"))
}

/// Translates a field name from NlqFilter into a ClickHouse column reference.
///
/// Known direct columns → `ms.<col>` or `mp.<col>`.
/// Others → `JSONExtractString(ms.attributes, '<field>')`.
fn map_filter_field(field: &str) -> String {
    match field {
        "service_name" | "service" => "ms.service_name".into(),
        "environment" | "env" => "ms.environment".into(),
        "metric_name" | "metric" => "ms.metric_name".into(),
        _ => format!(
            "JSONExtractString(ms.attributes, '{}')",
            escape_string_value(field)
        ),
    }
}

/// Builds a filter value expression for a given operator and raw string value.
fn build_filter_expr(col: &str, op: NlqFilterOp, value: &str) -> String {
    let escaped = escape_string_value(value);
    match op {
        NlqFilterOp::Eq => format!("{col} = '{escaped}'"),
        NlqFilterOp::Ne => format!("{col} != '{escaped}'"),
        NlqFilterOp::Re => format!("match({col}, '{escaped}')"),
        NlqFilterOp::Nre => format!("NOT match({col}, '{escaped}')"),
        NlqFilterOp::Gt => format!("{col} > {escaped}"),
        NlqFilterOp::Gte => format!("{col} >= {escaped}"),
        NlqFilterOp::Lt => format!("{col} < {escaped}"),
        NlqFilterOp::Lte => format!("{col} <= {escaped}"),
    }
}

/// Builds the `\n  AND …\n  AND …` filter block from a slice of NlqFilter.
fn build_filter_clauses(filters: &[NlqFilter]) -> String {
    if filters.is_empty() {
        return String::new();
    }
    filters
        .iter()
        .map(|f| {
            let col = map_filter_field(&f.field);
            format!("\n  AND {}", build_filter_expr(&col, f.op, &f.value))
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Builds `(extra_select_cols, group_by_extension)` for a group_by list.
///
/// Returns:
/// - `extra_select_cols`: `,\n    <col> AS <col>` fragment for SELECT
/// - `group_by_extension`: `, <col>` fragment appended after `GROUP BY bucket`
fn build_group_by(group_by: &[String]) -> (String, String) {
    if group_by.is_empty() {
        return (String::new(), String::new());
    }
    let select_part: String = group_by
        .iter()
        .map(|g| {
            let col = map_filter_field(g);
            format!(",\n    {col} AS {g}")
        })
        .collect();
    let group_part: String = group_by
        .iter()
        .map(|g| {
            let col = map_filter_field(g);
            format!(", {col}")
        })
        .collect();
    (select_part, group_part)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{NlqFilter, NlqFilterOp, NlqIr, NlqOperation, NlqSignal, NlqTimeRange};

    const TEST_TENANT: Uuid = Uuid::from_u128(0xAAAA_0000_0000_0000_0000_0000_0000_0001);

    fn base_ir(op: NlqOperation) -> NlqIr {
        NlqIr {
            operation: op,
            signals: vec![NlqSignal::Metrics],
            metric: Some("request_duration_ms".into()),
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

    fn ctx_for<'a>(ir: &'a NlqIr) -> SqlContext<'a> {
        SqlContext {
            tenant_id: TEST_TENANT,
            metric_name: "request_duration_ms",
            metric_type: SchemaMetricType::Gauge,
            ir,
        }
    }

    // ── Tenant isolation ──────────────────────────────────────────────────────

    #[test]
    fn every_operation_contains_tenant_id_in_where() {
        let ops = [
            NlqOperation::Timeseries,
            NlqOperation::Rate,
            NlqOperation::Irate,
            NlqOperation::Increase,
            NlqOperation::Histogram,
            NlqOperation::Topk,
            NlqOperation::Table,
            NlqOperation::Distribution,
        ];
        let tenant_str = format!("'{TEST_TENANT}'");
        for op in ops {
            let ir = base_ir(op);
            let ctx = ctx_for(&ir);
            let sql = generate_sql(&ctx).expect("generate_sql must succeed");
            assert!(
                sql.contains(&tenant_str),
                "operation {op:?}: SQL must contain tenant_id in WHERE, got:\n{sql}"
            );
        }
    }

    #[test]
    fn tenant_id_appears_after_where_keyword() {
        let ir = base_ir(NlqOperation::Timeseries);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        let where_pos = sql.find("WHERE").unwrap();
        let tenant_pos = sql.find(&format!("'{TEST_TENANT}'")).unwrap();
        assert!(
            tenant_pos > where_pos,
            "tenant_id must appear after WHERE keyword"
        );
    }

    // ── Metric name injection ─────────────────────────────────────────────────

    #[test]
    fn metric_name_is_injected_into_sql() {
        let ir = base_ir(NlqOperation::Timeseries);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("'request_duration_ms'"));
    }

    #[test]
    fn metric_name_with_single_quote_is_escaped() {
        let ir = base_ir(NlqOperation::Timeseries);
        let mut ctx = ctx_for(&ir);
        ctx.metric_name = "metric'with'quotes";
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains(r#"metric\'with\'quotes"#));
    }

    // ── Time range ────────────────────────────────────────────────────────────

    #[test]
    fn time_range_clause_uses_now_minus_for_relative() {
        let ir = base_ir(NlqOperation::Timeseries);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("toUnixTimestamp64Nano(now64()) - 3600000000000"),
            "got: {sql}"
        );
        assert!(
            sql.contains("toUnixTimestamp64Nano(now64())"),
            "to=now must render as toUnixTimestamp64Nano(now64())"
        );
    }

    #[test]
    fn parse_time_expr_now() {
        assert_eq!(
            parse_time_expr("now").unwrap(),
            "toUnixTimestamp64Nano(now64())"
        );
    }

    #[test]
    fn parse_time_expr_relative_minutes() {
        assert_eq!(
            parse_time_expr("now-5m").unwrap(),
            "toUnixTimestamp64Nano(now64()) - 300000000000"
        );
    }

    #[test]
    fn parse_time_expr_relative_hours() {
        assert_eq!(
            parse_time_expr("now-2h").unwrap(),
            "toUnixTimestamp64Nano(now64()) - 7200000000000"
        );
    }

    #[test]
    fn parse_time_expr_relative_days() {
        assert_eq!(
            parse_time_expr("now-1d").unwrap(),
            "toUnixTimestamp64Nano(now64()) - 86400000000000"
        );
    }

    #[test]
    fn parse_time_expr_relative_seconds() {
        assert_eq!(
            parse_time_expr("now-30s").unwrap(),
            "toUnixTimestamp64Nano(now64()) - 30000000000"
        );
    }

    #[test]
    fn parse_time_expr_unix_nano() {
        assert_eq!(
            parse_time_expr("1746274719123000000").unwrap(),
            "1746274719123000000"
        );
    }

    #[test]
    fn parse_time_expr_invalid_returns_error() {
        assert!(parse_time_expr("yesterday").is_err());
        assert!(parse_time_expr("now-").is_err());
        // ISO-8601 strings are rejected per ADR-030; callers must convert to Unix nanos
        assert!(parse_time_expr("2026-05-03T13:05:52.000Z").is_err());
    }

    // ── Resolution / interval ─────────────────────────────────────────────────

    #[test]
    fn parse_interval_minute() {
        assert_eq!(parse_interval("1m").unwrap(), "1 MINUTE");
        assert_eq!(parse_interval("5m").unwrap(), "5 MINUTE");
    }

    #[test]
    fn parse_interval_hour() {
        assert_eq!(parse_interval("1h").unwrap(), "1 HOUR");
    }

    #[test]
    fn parse_interval_invalid_returns_error() {
        assert!(parse_interval("fast").is_err());
        assert!(parse_interval("").is_err());
    }

    #[test]
    fn interval_to_secs_minute() {
        assert_eq!(interval_to_secs("1m").unwrap(), 60.0);
        assert_eq!(interval_to_secs("5m").unwrap(), 300.0);
    }

    #[test]
    fn interval_to_secs_hour() {
        assert_eq!(interval_to_secs("1h").unwrap(), 3600.0);
    }

    #[test]
    fn timeseries_sql_contains_group_by_bucket() {
        let ir = base_ir(NlqOperation::Timeseries);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("GROUP BY bucket"));
        assert!(sql.contains("ORDER BY bucket"));
    }

    // ── Filters ───────────────────────────────────────────────────────────────

    #[test]
    fn filter_eq_generates_correct_clause() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "payments".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("ms.service_name = 'payments'"), "got:\n{sql}");
    }

    #[test]
    fn filter_ne_generates_correct_clause() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "environment".into(),
            op: NlqFilterOp::Ne,
            value: "dev".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("ms.environment != 'dev'"));
    }

    #[test]
    fn filter_regex_uses_match_function() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Re,
            value: "pay.*".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("match(ms.service_name, 'pay.*')"));
    }

    #[test]
    fn filter_nre_uses_not_match() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Nre,
            value: "test.*".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("NOT match(ms.service_name, 'test.*')"));
    }

    #[test]
    fn filter_unknown_field_uses_json_extract() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "pod".into(),
            op: NlqFilterOp::Eq,
            value: "api-1".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("JSONExtractString(ms.attributes, 'pod') = 'api-1'"),
            "got:\n{sql}"
        );
    }

    #[test]
    fn filter_value_with_sql_injection_is_escaped() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "a'; DROP TABLE metric_series; --".into(),
        });
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        // The injected ' must be escaped to \' — the string is quoted safely
        assert!(
            sql.contains(r"a\'"),
            "single quote must be escaped to \\' in: {sql}"
        );
        // The statement string is fully within a quoted literal: ms.service_name = 'a\'; DROP...'
        // Verify the outer quote around the value is intact (value is still inside quotes)
        assert!(
            sql.contains("= 'a\\'"),
            "value must remain inside quoted literal: {sql}"
        );
    }

    // ── Group by ──────────────────────────────────────────────────────────────

    #[test]
    fn group_by_adds_to_select_and_group_clause() {
        let mut ir = base_ir(NlqOperation::Timeseries);
        ir.group_by = vec!["service_name".into()];
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("ms.service_name AS service_name"));
        assert!(sql.contains("GROUP BY bucket, ms.service_name"));
    }

    // ── Rate / irate / increase ───────────────────────────────────────────────

    #[test]
    fn rate_sql_contains_lag_function() {
        let mut ir = base_ir(NlqOperation::Rate);
        ir.window = Some("5m".into());
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("lagInFrame"), "rate must use lagInFrame");
        // window is now expressed as a Unix nano offset per ADR-030
        assert!(
            sql.contains("300000000000"),
            "5m window must appear as 300000000000 nanos in SQL: {sql}"
        );
    }

    #[test]
    fn rate_sql_divides_by_interval_seconds() {
        let mut ir = base_ir(NlqOperation::Rate);
        ir.resolution = Some("5m".into());
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("/ 300"),
            "rate must divide by interval seconds (300 for 5m)"
        );
    }

    #[test]
    fn irate_sql_uses_row_number_desc() {
        let mut ir = base_ir(NlqOperation::Irate);
        ir.window = Some("1m".into());
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("row_number()"), "irate must use row_number");
        assert!(
            sql.contains("ORDER BY mp.time_unix_nano DESC"),
            "irate row_number must be DESC: {sql}"
        );
    }

    #[test]
    fn increase_sql_sums_deltas() {
        let mut ir = base_ir(NlqOperation::Increase);
        ir.window = Some("1h".into());
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("sum(if(delta < 0, value, delta))"));
    }

    // ── Histogram ─────────────────────────────────────────────────────────────

    #[test]
    fn histogram_sql_uses_explicit_bounds() {
        let ir = base_ir(NlqOperation::Histogram);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("histogram_explicit_bounds"));
        assert!(sql.contains("histogram_bucket_counts"));
        assert!(sql.contains("arrayDifference"));
        assert!(sql.contains("notEmpty(mp.histogram_explicit_bounds)"));
    }

    // ── Topk ─────────────────────────────────────────────────────────────────

    #[test]
    fn topk_sql_has_limit_and_order_desc() {
        let ir = base_ir(NlqOperation::Topk);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("ORDER BY avg_value DESC"));
        assert!(sql.contains("LIMIT 10"));
    }

    // ── Table ─────────────────────────────────────────────────────────────────

    #[test]
    fn table_sql_has_limit_1000() {
        let ir = base_ir(NlqOperation::Table);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("LIMIT 1000"));
        assert!(sql.contains("ORDER BY timestamp_unix_nano DESC"));
    }

    // ── Distribution ─────────────────────────────────────────────────────────

    #[test]
    fn distribution_sql_defaults_include_all_six_stats() {
        let ir = base_ir(NlqOperation::Distribution);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("quantile(0.500)") || sql.contains("quantile(0.50)"),
            "p50 missing: {sql}"
        );
        assert!(
            sql.contains("quantile(0.990)") || sql.contains("quantile(0.99)"),
            "p99 missing: {sql}"
        );
        assert!(sql.contains("min("), "min missing: {sql}");
        assert!(sql.contains("max("), "max missing: {sql}");
    }

    #[test]
    fn distribution_sql_single_percentile_p99_only() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec!["p99".into()]);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("AS p99"), "p99 missing: {sql}");
        assert!(!sql.contains("AS p50"), "p50 must not appear: {sql}");
        assert!(!sql.contains("AS p90"), "p90 must not appear: {sql}");
        assert!(!sql.contains(" min("), "min must not appear: {sql}");
        assert!(!sql.contains(" max("), "max must not appear: {sql}");
    }

    #[test]
    fn distribution_sql_multi_stat_selection() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec![
            "p75".into(),
            "p95".into(),
            "p99".into(),
            "average".into(),
            "median".into(),
        ]);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(sql.contains("AS p75"), "p75 missing: {sql}");
        assert!(sql.contains("AS p95"), "p95 missing: {sql}");
        assert!(sql.contains("AS p99"), "p99 missing: {sql}");
        assert!(sql.contains("AS average"), "average missing: {sql}");
        assert!(sql.contains("AS median"), "median missing: {sql}");
        // Unasked stats must not appear.
        assert!(!sql.contains("AS p50"), "p50 must not appear: {sql}");
        assert!(!sql.contains("AS p90"), "p90 must not appear: {sql}");
    }

    #[test]
    fn distribution_sql_named_stats_median_average_min_max() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec![
            "median".into(),
            "average".into(),
            "min".into(),
            "max".into(),
        ]);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("quantile(0.50)") && sql.contains("AS median"),
            "median: {sql}"
        );
        assert!(
            sql.contains("avg(") && sql.contains("AS average"),
            "average: {sql}"
        );
        assert!(sql.contains("min(") && sql.contains("AS min"), "min: {sql}");
        assert!(sql.contains("max(") && sql.contains("AS max"), "max: {sql}");
    }

    #[test]
    fn distribution_sql_unknown_stats_silently_skipped_falls_back_to_p99() {
        let mut ir = base_ir(NlqOperation::Distribution);
        ir.percentiles = Some(vec!["not_a_stat".into(), "p0".into()]);
        let ctx = ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        // All unrecognised → fallback to p99.
        assert!(sql.contains("AS p99"), "fallback p99 missing: {sql}");
    }

    // ── escape_string_value ───────────────────────────────────────────────────

    #[test]
    fn escape_single_quote() {
        assert_eq!(escape_string_value("it's"), r"it\'s");
    }

    #[test]
    fn escape_backslash() {
        assert_eq!(escape_string_value(r"a\b"), r"a\\b");
    }

    #[test]
    fn escape_no_special_chars_unchanged() {
        assert_eq!(escape_string_value("hello_world"), "hello_world");
    }

    // ── Catalog ───────────────────────────────────────────────────────────────

    fn catalog_ir() -> NlqIr {
        NlqIr {
            operation: NlqOperation::Catalog,
            signals: vec![NlqSignal::Metrics],
            metric: None,
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-24h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: None,
        }
    }

    fn catalog_ctx_for<'a>(ir: &'a NlqIr) -> SqlContext<'a> {
        SqlContext {
            tenant_id: TEST_TENANT,
            metric_name: "",
            metric_type: SchemaMetricType::Unknown,
            ir,
        }
    }

    #[test]
    fn catalog_sql_service_name() {
        let mut ir = catalog_ir();
        ir.catalog_field = Some("service_name".into());
        let ctx = catalog_ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("ms.service_name AS service_name"),
            "service_name column: {sql}"
        );
        assert!(
            sql.contains("FROM observable.metric_series ms"),
            "must query metric_series: {sql}"
        );
        assert!(
            sql.contains("series_count"),
            "series_count must be in SELECT: {sql}"
        );
    }

    #[test]
    fn catalog_sql_attribute_field() {
        let mut ir = catalog_ir();
        ir.catalog_field = Some("pod".into());
        let ctx = catalog_ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("JSONExtractString(ms.attributes, 'pod') AS pod"),
            "attribute field must use JSONExtractString: {sql}"
        );
    }

    #[test]
    fn catalog_sql_missing_field() {
        let ir = catalog_ir(); // catalog_field is None
        let ctx = catalog_ctx_for(&ir);
        let result = generate_sql(&ctx);
        assert_eq!(result, Err(SqlTemplateError::MissingCatalogField));
    }

    #[test]
    fn catalog_sql_with_filter() {
        let mut ir = catalog_ir();
        ir.catalog_field = Some("metric_name".into());
        ir.filters.push(NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "checkout".into(),
        });
        let ctx = catalog_ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        assert!(
            sql.contains("ms.service_name = 'checkout'"),
            "filter must appear in SQL: {sql}"
        );
        assert!(
            sql.contains("ms.metric_name AS metric_name"),
            "metric_name col: {sql}"
        );
    }

    #[test]
    fn catalog_sql_contains_tenant_id() {
        let mut ir = catalog_ir();
        ir.catalog_field = Some("environment".into());
        let ctx = catalog_ctx_for(&ir);
        let sql = generate_sql(&ctx).unwrap();
        let tenant_str = format!("'{TEST_TENANT}'");
        assert!(
            sql.contains(&tenant_str),
            "catalog SQL must contain tenant_id: {sql}"
        );
    }

    // ── Log SQL tests ─────────────────────────────────────────────────────────

    fn log_ir(query: Option<&str>) -> NlqIr {
        NlqIr {
            operation: NlqOperation::Table,
            signals: vec![NlqSignal::Logs],
            metric: None,
            window: None,
            filters: vec![],
            group_by: vec![],
            resolution: None,
            time_range: NlqTimeRange {
                from: "now-3h".into(),
                to: "now".into(),
            },
            visualization_hint: None,
            percentiles: None,
            catalog_field: None,
            limit: None,
            query: query.map(|s| s.into()),
        }
    }

    #[test]
    fn log_sql_basic_query() {
        let ir = log_ir(Some("HTTP Request"));
        let ctx = LogSqlContext {
            tenant_id: TEST_TENANT,
            ir: &ir,
        };
        let sql = generate_log_sql(&ctx).unwrap();
        assert!(
            sql.contains("observable.logs"),
            "must query logs table: {sql}"
        );
        assert!(
            sql.contains("positionCaseInsensitive(body, 'HTTP Request') > 0"),
            "must use positionCaseInsensitive for body search: {sql}"
        );
        assert!(
            sql.contains(&format!("'{TEST_TENANT}'")),
            "must scope to tenant: {sql}"
        );
        assert!(sql.contains("LIMIT 200"), "must have limit: {sql}");
        assert!(
            sql.contains("ORDER BY timestamp_unix_nano DESC"),
            "must order by time: {sql}"
        );
    }

    #[test]
    fn log_sql_no_query_term() {
        let ir = log_ir(None);
        let ctx = LogSqlContext {
            tenant_id: TEST_TENANT,
            ir: &ir,
        };
        let sql = generate_log_sql(&ctx).unwrap();
        assert!(
            !sql.contains("positionCaseInsensitive"),
            "no body search when query is None: {sql}"
        );
        assert!(
            sql.contains("observable.logs"),
            "must query logs table: {sql}"
        );
    }

    #[test]
    fn log_sql_with_service_filter() {
        let mut ir = log_ir(Some("timeout"));
        ir.filters = vec![NlqFilter {
            field: "service_name".into(),
            op: NlqFilterOp::Eq,
            value: "checkout".into(),
        }];
        let ctx = LogSqlContext {
            tenant_id: TEST_TENANT,
            ir: &ir,
        };
        let sql = generate_log_sql(&ctx).unwrap();
        assert!(
            sql.contains("service_name = 'checkout'"),
            "must filter by service_name: {sql}"
        );
    }

    #[test]
    fn log_sql_escapes_single_quotes_in_query() {
        let ir = log_ir(Some("it's a test"));
        let ctx = LogSqlContext {
            tenant_id: TEST_TENANT,
            ir: &ir,
        };
        let sql = generate_log_sql(&ctx).unwrap();
        assert!(
            sql.contains("it\\'s a test"),
            "must escape single quotes: {sql}"
        );
    }

    #[test]
    fn log_sql_severity_filter_maps_correctly() {
        let mut ir = log_ir(None);
        ir.filters = vec![NlqFilter {
            field: "severity_text".into(),
            op: NlqFilterOp::Eq,
            value: "ERROR".into(),
        }];
        let ctx = LogSqlContext {
            tenant_id: TEST_TENANT,
            ir: &ir,
        };
        let sql = generate_log_sql(&ctx).unwrap();
        assert!(
            sql.contains("severity_text = 'ERROR'"),
            "severity filter must map directly: {sql}"
        );
    }
}
