//! Change-event listing query planning for the browser-local playground.
//! Production's `services/query-api/src/change_events.rs::list_change_events`
//! runs parameterized SQL against a Postgres `change_events` table and is
//! untouched here — this is a new DuckDB dialect renderer for the
//! playground's local table of the same name (see `engineWorker.ts`).

use crate::sql_templates::escape_string_value;

/// Renders a DuckDB-flavored filtered/limited query against the
/// playground's local `change_events` table, mirroring
/// `list_change_events`'s optional-filter shape (`service_name`,
/// `environment`, `event_type`, time range) and default/max limit
/// (`unwrap_or(50).min(200)`, applied by the caller before calling this).
pub fn render_change_events_duckdb(
    service_name: Option<&str>,
    environment: Option<&str>,
    event_type: Option<&str>,
    from_ns: u64,
    to_ns: u64,
    limit: u32,
) -> String {
    let mut where_clauses = vec![
        format!("occurred_at_unix_nano >= {from_ns}"),
        format!("occurred_at_unix_nano <= {to_ns}"),
    ];
    if let Some(svc) = service_name {
        where_clauses.push(format!("service_name = '{}'", escape_string_value(svc)));
    }
    if let Some(env) = environment {
        where_clauses.push(format!("environment = '{}'", escape_string_value(env)));
    }
    if let Some(et) = event_type {
        where_clauses.push(format!("event_type = '{}'", escape_string_value(et)));
    }
    let where_sql = where_clauses.join(" AND ");

    format!(
        "SELECT \
           change_event_id, \
           event_type, \
           service_name, \
           environment, \
           title, \
           description, \
           occurred_at_unix_nano, \
           source \
         FROM change_events \
         WHERE {where_sql} \
         ORDER BY occurred_at_unix_nano DESC \
         LIMIT {limit}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_includes_order_by_and_limit() {
        let sql = render_change_events_duckdb(None, None, None, 0, 1000, 50);
        assert!(sql.contains("FROM change_events"));
        assert!(sql.contains("ORDER BY occurred_at_unix_nano DESC"));
        assert!(sql.contains("LIMIT 50"));
    }

    #[test]
    fn render_includes_all_optional_filters_when_present() {
        let sql = render_change_events_duckdb(
            Some("checkout"),
            Some("production"),
            Some("feature_flag"),
            0,
            1000,
            50,
        );
        assert!(sql.contains("service_name = 'checkout'"));
        assert!(sql.contains("environment = 'production'"));
        assert!(sql.contains("event_type = 'feature_flag'"));
    }

    #[test]
    fn render_omits_optional_filters_when_absent() {
        let sql = render_change_events_duckdb(None, None, None, 0, 1000, 50);
        assert!(!sql.contains("service_name ="));
        assert!(!sql.contains("environment ="));
        assert!(!sql.contains("event_type ="));
    }

    #[test]
    fn render_escapes_filter_values() {
        let sql = render_change_events_duckdb(Some("o'brien"), None, None, 0, 1000, 50);
        assert!(sql.contains("service_name = 'o\\'brien'"));
    }
}
