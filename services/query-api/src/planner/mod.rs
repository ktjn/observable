use crate::discovery::TopologyParams;
use crate::logs::LogSearchParams;
use crate::traces::{SearchParams as TraceSearchParams, SELECT_COLS};
use std::collections::HashMap;

pub struct LogQueryPlan {
    pub count_sql: String,
    pub facet_plans: HashMap<String, FacetPlan>,
    pub logs_sql: String,
    pub limit: u32,
}

pub struct FacetPlan {
    pub sql: String,
}

pub struct TopologyPlan {
    pub sql: String,
}

pub struct TraceSearchPlan {
    pub count_sql: String,
    pub spans_sql: String,
    pub limit: u64,
}

pub struct LogHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}

pub struct ResponseTimeHistogramPlan {
    pub sql: String,
    pub from_ns: u64,
    pub interval_ns: u64,
}

#[derive(Clone, Default)]
pub struct QueryPlanner;

impl QueryPlanner {
    pub fn plan_trace_search(&self, params: &TraceSearchParams) -> TraceSearchPlan {
        let where_clause = trace_search_where_clause(params);
        let count_sql =
            format!("SELECT count(DISTINCT trace_id) FROM observable.spans {where_clause}");

        let latest_trace_ids_sql = format!(
            "(SELECT tenant_id, trace_id, max(start_time_unix_nano) FROM observable.spans {where_clause} GROUP BY tenant_id, trace_id ORDER BY max(start_time_unix_nano) DESC LIMIT ?)"
        );

        let spans_sql = format!(
            "SELECT {SELECT_COLS} FROM observable.spans \
             WHERE (tenant_id, trace_id, start_time_unix_nano) IN {latest_trace_ids_sql} \
             ORDER BY start_time_unix_nano DESC"
        );

        TraceSearchPlan {
            count_sql,
            spans_sql,
            limit: params.limit.unwrap_or(50).min(500) as u64,
        }
    }

    pub fn plan_log_search(&self, params: &LogSearchParams) -> LogQueryPlan {
        let where_clause = log_search_where_clause(params);
        let count_sql = format!("SELECT count() FROM observable.logs {where_clause}");

        let mut facet_plans = HashMap::new();
        if let Some(facets_str) = &params.facets {
            for field in requested_log_facets(facets_str) {
                let facet_sql = format!(
                    "SELECT toString({field}) as value, count() as count FROM observable.logs {where_clause} GROUP BY {field} ORDER BY count DESC LIMIT 10"
                );
                facet_plans.insert(field.to_string(), FacetPlan { sql: facet_sql });
            }
        }

        let logs_sql = format!(
            "SELECT ?fields FROM observable.logs {where_clause} ORDER BY timestamp_unix_nano DESC LIMIT ?"
        );

        LogQueryPlan {
            count_sql,
            facet_plans,
            logs_sql,
            limit: params.limit.unwrap_or(50).min(500),
        }
    }

    pub fn plan_topology(&self, params: &TopologyParams) -> TopologyPlan {
        let mut branch1 = "SELECT \
                parent.service_name AS caller, \
                child.service_name AS callee, \
                count() AS request_count, \
                countIf(child.status_code = 'ERROR') AS error_count, \
                quantile(0.95)(child.duration_ns) AS p95_latency_ns \
            FROM observable.spans AS child \
            INNER JOIN spans AS parent ON child.parent_span_id = parent.span_id AND child.trace_id = parent.trace_id \
            WHERE child.tenant_id = ? AND parent.tenant_id = ? \
              AND child.service_name != parent.service_name \
              AND child.start_time_unix_nano >= ?"
            .to_string();

        if params.environment.is_some() {
            branch1.push_str(" AND child.environment = ? AND parent.environment = ?");
        }
        if params.service.is_some() {
            branch1.push_str(" AND (child.service_name = ? OR parent.service_name = ?)");
        }
        branch1.push_str(" GROUP BY caller, callee");

        // Branch 2: trace-level co-occurrence. Catches cross-service relationships
        // that span parent_span_id gaps (e.g. async handoff). Counts ordered
        // span-time pairs, not actual call events — intentional approximation.
        let mut branch2 = "SELECT \
                s1.service_name AS caller, \
                s2.service_name AS callee, \
                count() AS request_count, \
                countIf(s2.status_code = 'ERROR') AS error_count, \
                quantile(0.95)(s2.duration_ns) AS p95_latency_ns \
            FROM observable.spans AS s1 \
            INNER JOIN spans AS s2 ON s1.trace_id = s2.trace_id \
            WHERE s1.tenant_id = ? AND s2.tenant_id = ? \
              AND s1.service_name != s2.service_name \
              AND s1.start_time_unix_nano <= s2.start_time_unix_nano \
              AND s1.start_time_unix_nano >= ?"
            .to_string();

        if params.environment.is_some() {
            branch2.push_str(" AND s1.environment = ? AND s2.environment = ?");
        }
        if params.service.is_some() {
            branch2.push_str(" AND (s1.service_name = ? OR s2.service_name = ?)");
        }
        branch2.push_str(" GROUP BY caller, callee");

        let sql = format!(
            "SELECT caller, callee, \
                max(request_count) AS request_count, \
                max(error_count) AS error_count, \
                max(p95_latency_ns) AS p95_latency_ns \
            FROM ({branch1} UNION ALL {branch2}) \
            GROUP BY caller, callee \
            ORDER BY request_count DESC"
        );

        TopologyPlan { sql }
    }

    pub fn plan_log_histogram(
        &self,
        from_ns: u64,
        to_ns: u64,
        service: Option<&str>,
        bucket_count: u32,
    ) -> LogHistogramPlan {
        let range_ns = to_ns.saturating_sub(from_ns).max(1);
        let interval_ns = (range_ns / bucket_count as u64).max(1);

        let mut where_clause = "WHERE tenant_id = ? \
             AND timestamp_unix_nano >= ? \
             AND timestamp_unix_nano <= ?"
            .to_string();
        if service.is_some() {
            where_clause.push_str(" AND service_name = ?");
        }

        let sql = format!(
            "SELECT \
               intDiv(timestamp_unix_nano - ?, ?) AS bucket_idx, \
               severity_number, \
               count() AS cnt \
             FROM observable.logs {where_clause} \
             GROUP BY bucket_idx, severity_number \
             ORDER BY bucket_idx ASC"
        );

        LogHistogramPlan {
            sql,
            from_ns,
            interval_ns,
        }
    }

    pub fn plan_trace_histogram(
        &self,
        from_ns: u64,
        to_ns: u64,
        service: Option<&str>,
        bucket_count: u32,
    ) -> LogHistogramPlan {
        let range_ns = to_ns.saturating_sub(from_ns).max(1);
        let interval_ns = (range_ns / bucket_count as u64).max(1);

        let mut where_clause = "WHERE tenant_id = ? \
             AND start_time_unix_nano >= ? \
             AND start_time_unix_nano <= ?"
            .to_string();
        if service.is_some() {
            where_clause.push_str(" AND service_name = ?");
        }

        let sql = format!(
            "SELECT \
               intDiv(start_time_unix_nano - ?, ?) AS bucket_idx, \
               toInt32(1) as dummy_severity, \
               count(DISTINCT trace_id) AS cnt \
             FROM observable.spans {where_clause} \
             GROUP BY bucket_idx \
             ORDER BY bucket_idx ASC"
        );

        LogHistogramPlan {
            sql,
            from_ns,
            interval_ns,
        }
    }

    pub fn plan_response_time_histogram(
        &self,
        from_ns: u64,
        to_ns: u64,
        bucket_count: u32,
    ) -> ResponseTimeHistogramPlan {
        let range_ns = to_ns.saturating_sub(from_ns).max(1);
        let interval_ns = (range_ns / bucket_count as u64).max(1);

        let sql = "SELECT \
           intDiv(start_time_unix_nano - ?, ?) AS bucket_idx, \
           quantile(0.50)(duration_ns) AS p50_ns, \
           quantile(0.95)(duration_ns) AS p95_ns, \
           count() AS span_count \
         FROM observable.spans \
         WHERE tenant_id = ? \
           AND service_name = ? \
           AND start_time_unix_nano >= ? \
           AND start_time_unix_nano <= ? \
         GROUP BY bucket_idx \
         ORDER BY bucket_idx ASC"
            .to_string();

        ResponseTimeHistogramPlan { sql, from_ns, interval_ns }
    }
}

fn trace_search_where_clause(params: &TraceSearchParams) -> String {
    let mut where_clause = "WHERE tenant_id = ?".to_string();
    if params.from.is_some() || params.lookback_minutes.is_some() {
        where_clause.push_str(" AND start_time_unix_nano >= ?");
    }
    if params.to.is_some() {
        where_clause.push_str(" AND start_time_unix_nano <= ?");
    }
    if params.service.is_some() {
        where_clause.push_str(" AND service_name = ?");
    }
    where_clause
}

fn log_search_where_clause(params: &LogSearchParams) -> String {
    let mut where_clause = "WHERE tenant_id = ?".to_string();
    if params.from.is_some() {
        where_clause.push_str(" AND timestamp_unix_nano >= ?");
    }
    if params.to.is_some() {
        where_clause.push_str(" AND timestamp_unix_nano <= ?");
    }
    if params.service.is_some() {
        where_clause.push_str(" AND service_name = ?");
    }
    if params.severity.is_some() {
        where_clause.push_str(" AND severity_number >= ?");
    }
    if params.trace_id.is_some() {
        where_clause.push_str(" AND trace_id = ?");
    }
    if params.span_id.is_some() {
        where_clause.push_str(" AND span_id = ?");
    }
    where_clause
}

fn requested_log_facets(facets_str: &str) -> impl Iterator<Item = &str> {
    facets_str.split(',').map(str::trim).filter(|field| {
        matches!(
            *field,
            "service_name" | "severity_number" | "environment" | "host_id"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono::Utc;

    fn params() -> LogSearchParams {
        LogSearchParams {
            service: None,
            severity: None,
            trace_id: None,
            span_id: None,
            limit: None,
            facets: None,
            from: None,
            to: None,
        }
    }

    #[test]
    fn log_search_plan_matches_unfiltered_endpoint_sql() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_search(&params());

        assert_eq!(
            plan.count_sql,
            "SELECT count() FROM observable.logs WHERE tenant_id = ?"
        );
        assert_eq!(
            plan.logs_sql,
            "SELECT ?fields FROM observable.logs WHERE tenant_id = ? ORDER BY timestamp_unix_nano DESC LIMIT ?"
        );
        assert_eq!(plan.limit, 50);
        assert!(plan.facet_plans.is_empty());
    }

    #[test]
    fn log_search_plan_preserves_filter_order_and_limit_clamp() {
        let planner = QueryPlanner;
        let mut params = params();
        params.service = Some("checkout".into());
        params.severity = Some(13);
        params.trace_id = Some("trace-1".into());
        params.span_id = Some("span-1".into());
        params.from = Some(Utc.with_ymd_and_hms(2026, 4, 29, 12, 0, 0).unwrap());
        params.limit = Some(900);

        let plan = planner.plan_log_search(&params);

        assert_eq!(
            plan.count_sql,
            "SELECT count() FROM observable.logs WHERE tenant_id = ? AND timestamp_unix_nano >= ? AND service_name = ? AND severity_number >= ? AND trace_id = ? AND span_id = ?"
        );
        assert_eq!(
            plan.logs_sql,
            "SELECT ?fields FROM observable.logs WHERE tenant_id = ? AND timestamp_unix_nano >= ? AND service_name = ? AND severity_number >= ? AND trace_id = ? AND span_id = ? ORDER BY timestamp_unix_nano DESC LIMIT ?"
        );
        assert_eq!(plan.limit, 500);
    }

    #[test]
    fn log_search_plan_keeps_only_allowed_facets() {
        let planner = QueryPlanner;
        let mut params = params();
        params.facets = Some("service_name, invalid_field, host_id, severity_number".into());

        let plan = planner.plan_log_search(&params);

        assert!(plan.facet_plans.contains_key("service_name"));
        assert!(plan.facet_plans.contains_key("host_id"));
        assert!(plan.facet_plans.contains_key("severity_number"));
        assert!(!plan.facet_plans.contains_key("invalid_field"));
        assert_eq!(plan.facet_plans.len(), 3);

        // Verify toString conversion for type safety
        let sev_plan = &plan.facet_plans["severity_number"];
        assert!(sev_plan
            .sql
            .contains("SELECT toString(severity_number) as value"));
    }

    #[test]
    fn topology_plan_includes_tenant_and_time_filters() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: None,
            lookback_minutes: None,
            service: None,
        };

        let plan = planner.plan_topology(&params);

        assert!(plan
            .sql
            .contains("WHERE child.tenant_id = ? AND parent.tenant_id = ?"));
        assert!(plan.sql.contains("AND child.start_time_unix_nano >= ?"));
        assert!(plan.sql.contains("GROUP BY caller, callee"));
        assert!(plan.sql.contains("ORDER BY request_count DESC"));
    }

    #[test]
    fn topology_plan_can_filter_by_service() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: None,
            lookback_minutes: None,
            service: Some("checkout".into()),
        };

        let plan = planner.plan_topology(&params);

        assert!(plan
            .sql
            .contains("AND (child.service_name = ? OR parent.service_name = ?)"));
    }

    #[test]
    fn topology_plan_includes_union_and_cooccurrence_branch() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: None,
            lookback_minutes: None,
            service: None,
        };

        let plan = planner.plan_topology(&params);

        assert!(
            plan.sql.contains("UNION ALL"),
            "SQL should contain UNION ALL"
        );
        assert!(
            plan.sql
                .contains("s1.start_time_unix_nano <= s2.start_time_unix_nano"),
            "SQL should contain co-occurrence time ordering"
        );
        assert!(
            plan.sql.contains("max(request_count) AS request_count"),
            "SQL should contain outer dedup aggregation"
        );
    }

    #[test]
    fn topology_plan_with_environment_filter_applies_to_both_branches() {
        let planner = QueryPlanner;
        let params = TopologyParams {
            environment: Some("prod".into()),
            lookback_minutes: None,
            service: None,
        };

        let plan = planner.plan_topology(&params);

        assert!(
            plan.sql
                .contains("AND child.environment = ? AND parent.environment = ?"),
            "Branch 1 should have env filter"
        );
        assert!(
            plan.sql
                .contains("AND s1.environment = ? AND s2.environment = ?"),
            "Branch 2 should have env filter"
        );
    }

    #[test]
    fn log_histogram_sql_uses_bind_params_to_avoid_int64_schema_mismatch() {
        // from_ns and interval_ns are passed as bind parameters (?) rather than
        // interpolated literals. This is architecturally consistent with every
        // other planner query and avoids the Int64 schema mismatch without needing
        // toUInt64() casts — the handler fetches as (i64, i32, u64) to match the
        // actual ClickHouse return type.
        let planner = QueryPlanner;
        let plan = planner.plan_log_histogram(1_000_000u64, 2_000_000u64, None, 30);

        assert!(
            plan.sql.contains("intDiv(timestamp_unix_nano - ?, ?)"),
            "intDiv must use bind parameters for from_ns and interval_ns; sql: {}",
            plan.sql
        );
        assert!(
            !plan.sql.contains("toUInt64"),
            "SQL must not contain toUInt64 casts — bind params are used instead; sql: {}",
            plan.sql
        );
    }

    #[test]
    fn trace_search_plan_counts_total_without_limit() {
        let planner = QueryPlanner;
        let mut params = TraceSearchParams {
            service: Some("checkout".into()),
            limit: Some(10),
            facets: None,
            from: None,
            to: None,
            lookback_minutes: None,
        };

        let plan = planner.plan_trace_search(&params);

        assert_eq!(
            plan.count_sql,
            "SELECT count(DISTINCT trace_id) FROM observable.spans WHERE tenant_id = ? AND service_name = ?"
        );
        assert!(!plan.count_sql.contains("LIMIT"));

        params.service = None;
        let plan = planner.plan_trace_search(&params);
        assert_eq!(
            plan.count_sql,
            "SELECT count(DISTINCT trace_id) FROM observable.spans WHERE tenant_id = ?"
        );
    }

    #[test]
    fn trace_search_plan_orders_by_latest_span_per_trace() {
        let planner = QueryPlanner;
        let params = TraceSearchParams {
            service: None,
            limit: Some(900),
            facets: None,
            from: None,
            to: None,
            lookback_minutes: None,
        };

        let plan = planner.plan_trace_search(&params);

        assert_eq!(plan.limit, 500);
        assert!(plan.spans_sql.contains("max(start_time_unix_nano)"));
        assert!(plan.spans_sql.contains("GROUP BY tenant_id, trace_id"));
        assert!(plan
            .spans_sql
            .contains("ORDER BY max(start_time_unix_nano) DESC LIMIT ?"));
        assert!(!plan.spans_sql.contains("SELECT DISTINCT trace_id"));
        assert!(plan
            .spans_sql
            .contains("WHERE (tenant_id, trace_id, start_time_unix_nano) IN"));
    }

    #[test]
    fn log_histogram_plan_sql_shape() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_histogram(0, 3_000_000_000, None, 30);

        assert!(plan.sql.contains("FROM observable.logs"));
        assert!(plan.sql.contains("intDiv(timestamp_unix_nano - ?, ?)"));
        assert!(plan.sql.contains("GROUP BY bucket_idx, severity_number"));
        assert!(plan.sql.contains("ORDER BY bucket_idx ASC"));
        assert!(plan.sql.contains("WHERE tenant_id = ?"));
        assert!(plan.sql.contains("AND timestamp_unix_nano >= ?"));
        assert!(plan.sql.contains("AND timestamp_unix_nano <= ?"));
    }

    #[test]
    fn log_histogram_interval_calculated_from_range() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_histogram(0, 3_000_000_000, None, 30);

        assert_eq!(plan.interval_ns, 100_000_000);
        assert_eq!(plan.from_ns, 0);
    }

    #[test]
    fn log_histogram_interval_clamps_to_one() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_histogram(1_000_000, 1_000_000, None, 30);

        assert_eq!(plan.interval_ns, 1);
    }

    #[test]
    fn log_histogram_with_service_filter() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_histogram(0, 3_000_000_000, Some("checkout"), 30);

        assert!(plan.sql.contains("AND service_name = ?"));
    }

    #[test]
    fn log_histogram_bucket_count_respected() {
        let planner = QueryPlanner;
        let plan_a = planner.plan_log_histogram(0, 60_000_000_000, None, 60);
        let plan_b = planner.plan_log_histogram(0, 60_000_000_000, None, 30);

        assert_eq!(plan_a.interval_ns, 1_000_000_000);
        assert_eq!(plan_b.interval_ns, 2_000_000_000);
        assert_eq!(plan_b.interval_ns, plan_a.interval_ns * 2);
    }

    #[test]
    fn plan_response_time_histogram_divides_range_into_equal_intervals() {
        let planner = QueryPlanner;
        let from_ns = 0u64;
        let to_ns = 60 * 1_000_000_000u64; // 60 seconds in ns
        let plan = planner.plan_response_time_histogram(from_ns, to_ns, 60);
        assert_eq!(plan.interval_ns, 1_000_000_000, "interval should be 1 second");
        assert_eq!(plan.from_ns, 0);
        assert!(plan.sql.contains("quantile(0.50)"), "sql must include P50");
        assert!(plan.sql.contains("quantile(0.95)"), "sql must include P95");
        assert!(plan.sql.contains("service_name = ?"), "sql must filter by service");
    }
}
