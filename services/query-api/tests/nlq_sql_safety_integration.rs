// Unit-level SQL-safety tests — no real DB needed; validates SQL generation safety.
//
// These tests exercise the public `build_filter_expr_checked` API to confirm that:
//   1. Numeric-operator filters reject non-numeric values (prevents SQL injection
//      via numeric comparison operators).
//   2. Regex-operator filters reject overlong patterns (prevents ReDoS).
//   3. String-operator filters safely escape injection payloads rather than rejecting
//      them (the value is always kept inside a quoted literal).

#[test]
fn numeric_filter_rejects_sql_injection_attempt() {
    use domain::NlqFilterOp;
    use query_api::sql_templates::{build_filter_expr_checked, SqlTemplateError};

    let result = build_filter_expr_checked("duration_ms", NlqFilterOp::Gt, "0 OR 1=1");
    assert!(
        matches!(result, Err(SqlTemplateError::InvalidFilterValue(_))),
        "numeric operator must reject non-numeric value, got: {result:?}"
    );
}

#[test]
fn regex_filter_rejects_overlong_pattern() {
    use domain::NlqFilterOp;
    use query_api::sql_templates::{build_filter_expr_checked, SqlTemplateError};

    let long_pattern = "a".repeat(257);
    let result = build_filter_expr_checked("service_name", NlqFilterOp::Re, &long_pattern);
    assert!(
        matches!(result, Err(SqlTemplateError::InvalidFilterValue(_))),
        "regex operator must reject patterns longer than 256 chars, got: {result:?}"
    );
}

#[test]
fn string_filter_accepts_injected_payload_as_literal() {
    use domain::NlqFilterOp;
    use query_api::sql_templates::build_filter_expr_checked;

    // Injection attempt in a string equality filter — should be safely escaped, not rejected.
    let result = build_filter_expr_checked("service_name", NlqFilterOp::Eq, "checkout' OR '1'='1");
    let sql = result.expect("string filters must accept any value after escaping");
    // The injected single quote must be backslash-escaped.
    assert!(
        sql.contains("\\'"),
        "injected quote must be backslash-escaped in: {sql}"
    );
    // The raw injection payload must not appear verbatim in the output.
    assert!(
        !sql.contains("OR '1'='1"),
        "raw injection payload must not appear in output: {sql}"
    );
}
