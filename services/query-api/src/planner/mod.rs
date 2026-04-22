use crate::logs::LogSearchParams;
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

#[derive(Clone, Default)]
pub struct QueryPlanner;

impl QueryPlanner {
    pub fn plan_log_search(&self, params: &LogSearchParams) -> LogQueryPlan {
        let where_clause = log_search_where_clause(params);
        let count_sql = format!("SELECT count() FROM logs {where_clause}");

        let mut facet_plans = HashMap::new();
        if let Some(facets_str) = &params.facets {
            for field in requested_log_facets(facets_str) {
                let facet_sql = format!(
                    "SELECT {field} as value, count() as count FROM logs {where_clause} GROUP BY {field} ORDER BY count DESC LIMIT 10"
                );
                facet_plans.insert(field.to_string(), FacetPlan { sql: facet_sql });
            }
        }

        let logs_sql = format!(
            "SELECT ?fields FROM logs {where_clause} ORDER BY timestamp_unix_nano DESC LIMIT ?"
        );

        LogQueryPlan {
            count_sql,
            facet_plans,
            logs_sql,
            limit: params.limit.unwrap_or(50).min(500),
        }
    }
}

fn log_search_where_clause(params: &LogSearchParams) -> String {
    let mut where_clause = "WHERE tenant_id = ?".to_string();
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

    fn params() -> LogSearchParams {
        LogSearchParams {
            service: None,
            severity: None,
            trace_id: None,
            span_id: None,
            limit: None,
            facets: None,
        }
    }

    #[test]
    fn log_search_plan_matches_unfiltered_endpoint_sql() {
        let planner = QueryPlanner;
        let plan = planner.plan_log_search(&params());

        assert_eq!(
            plan.count_sql,
            "SELECT count() FROM logs WHERE tenant_id = ?"
        );
        assert_eq!(
            plan.logs_sql,
            "SELECT ?fields FROM logs WHERE tenant_id = ? ORDER BY timestamp_unix_nano DESC LIMIT ?"
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
        params.limit = Some(900);

        let plan = planner.plan_log_search(&params);

        assert_eq!(
            plan.count_sql,
            "SELECT count() FROM logs WHERE tenant_id = ? AND service_name = ? AND severity_number >= ? AND trace_id = ? AND span_id = ?"
        );
        assert_eq!(
            plan.logs_sql,
            "SELECT ?fields FROM logs WHERE tenant_id = ? AND service_name = ? AND severity_number >= ? AND trace_id = ? AND span_id = ? ORDER BY timestamp_unix_nano DESC LIMIT ?"
        );
        assert_eq!(plan.limit, 500);
    }

    #[test]
    fn log_search_plan_keeps_only_allowed_facets() {
        let planner = QueryPlanner;
        let mut params = params();
        params.facets = Some("service_name, invalid_field, host_id".into());

        let plan = planner.plan_log_search(&params);

        assert!(plan.facet_plans.contains_key("service_name"));
        assert!(plan.facet_plans.contains_key("host_id"));
        assert!(!plan.facet_plans.contains_key("invalid_field"));
        assert_eq!(plan.facet_plans.len(), 2);
    }
}
