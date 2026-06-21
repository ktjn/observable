use query_api::alerts::{
    CreateRuleError, CreateRuleRequest, create_alert_rule, list_alert_rules, silence_alert_rule,
};
use uuid::Uuid;

#[tokio::test]
async fn list_rules_returns_seeded_dev_rule() {
    let pool = test_support::postgres::shared_pool().await;
    let dev_tenant = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();

    let rules = list_alert_rules(&pool, dev_tenant).await.unwrap();

    assert!(
        !rules.is_empty(),
        "dev tenant must have at least one seeded rule"
    );
    assert!(
        rules.iter().any(|r| r.name == "High error rate"),
        "seeded 'High error rate' rule must be present"
    );
    let seeded = rules.iter().find(|r| r.name == "High error rate").unwrap();
    assert_eq!(seeded.metric_name, "error_rate");
    assert_eq!(seeded.operator, "gt");
    assert!(!seeded.silenced);
    assert!(!seeded.firing);
    assert_eq!(seeded.state, "ok");
}

#[tokio::test]
async fn create_rule_appears_in_list() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Test latency rule".into(),
        metric_name: "p95_latency_ms".into(),
        operator: "gt".into(),
        threshold: 500.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: None,
        service_name: None,
        window_secs: None,
        baseline_offset_secs: None,
        threshold_percent: None,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.name, "Test latency rule");
    assert_eq!(created.metric_name, "p95_latency_ms");
    assert_eq!(created.operator, "gt");
    assert!((created.threshold - 500.0).abs() < f64::EPSILON);
    assert_eq!(created.severity, "warning");
    assert!(!created.silenced);
    assert!(!created.firing);
    assert!(created.auto_trigger_incident);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules.iter().any(|r| r.rule_id == created.rule_id),
        "created rule must appear in list"
    );
}

#[tokio::test]
async fn silence_toggle_updates_silenced_flag() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Silenceable rule".into(),
        metric_name: "cpu_usage".into(),
        operator: "gt".into(),
        threshold: 0.9,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: None,
        service_name: None,
        window_secs: None,
        baseline_offset_secs: None,
        threshold_percent: None,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();
    assert!(!created.silenced);

    let silenced = silence_alert_rule(&pool, tenant, created.rule_id, true)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(silenced.silenced);

    let unsilenced = silence_alert_rule(&pool, tenant, created.rule_id, false)
        .await
        .unwrap()
        .expect("rule must exist for tenant");
    assert!(!unsilenced.silenced);
}

#[tokio::test]
async fn silence_returns_none_for_cross_tenant_rule() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "test rule".into(),
        metric_name: "test_metric".into(),
        operator: "gt".into(),
        threshold: 10.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: None,
        service_name: None,
        window_secs: None,
        baseline_offset_secs: None,
        threshold_percent: None,
    };

    let created = create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let result = silence_alert_rule(&pool, tenant_b, created.rule_id, true)
        .await
        .unwrap();
    assert!(
        result.is_none(),
        "tenant B must not be able to silence tenant A's rule"
    );

    let rules = list_alert_rules(&pool, tenant_a).await.unwrap();
    let rule = rules.iter().find(|r| r.rule_id == created.rule_id).unwrap();
    assert!(
        !rule.silenced,
        "rule must remain unsilenced after cross-tenant attempt"
    );
}

#[tokio::test]
async fn list_rules_does_not_return_other_tenant_rules() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant_a = Uuid::new_v4();
    let tenant_b = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "test rule".into(),
        metric_name: "test_metric".into(),
        operator: "gt".into(),
        threshold: 10.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: None,
        service_name: None,
        window_secs: None,
        baseline_offset_secs: None,
        threshold_percent: None,
    };

    create_alert_rule(&pool, tenant_a, &req).await.unwrap();

    let tenant_b_rules = list_alert_rules(&pool, tenant_b).await.unwrap();
    assert!(
        tenant_b_rules.is_empty(),
        "tenant B must not see tenant A's rules"
    );
}

#[tokio::test]
async fn list_rules_reports_pending_active_resolved_and_silenced_states() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let pending = create_alert_rule(
        &pool,
        tenant,
        &CreateRuleRequest {
            name: "Pending rule".into(),
            metric_name: "pending_metric".into(),
            operator: "gt".into(),
            threshold: 1.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: None,
            service_name: None,
            window_secs: None,
            baseline_offset_secs: None,
            threshold_percent: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'pending', 2.0)",
    )
    .bind(pending.rule_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let active = create_alert_rule(
        &pool,
        tenant,
        &CreateRuleRequest {
            name: "Active rule".into(),
            metric_name: "active_metric".into(),
            operator: "gt".into(),
            threshold: 1.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: None,
            service_name: None,
            window_secs: None,
            baseline_offset_secs: None,
            threshold_percent: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value) \
         VALUES ($1, $2, 'active', 2.0)",
    )
    .bind(active.rule_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let resolved = create_alert_rule(
        &pool,
        tenant,
        &CreateRuleRequest {
            name: "Resolved rule".into(),
            metric_name: "resolved_metric".into(),
            operator: "gt".into(),
            threshold: 1.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: None,
            service_name: None,
            window_secs: None,
            baseline_offset_secs: None,
            threshold_percent: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO alert_firings (rule_id, tenant_id, state, value, resolved_at) \
         VALUES ($1, $2, 'resolved', 0.5, NOW())",
    )
    .bind(resolved.rule_id)
    .bind(tenant)
    .execute(&pool)
    .await
    .unwrap();

    let silenced = create_alert_rule(
        &pool,
        tenant,
        &CreateRuleRequest {
            name: "Silenced rule".into(),
            metric_name: "silenced_metric".into(),
            operator: "gt".into(),
            threshold: 1.0,
            notification_channels: None,
            auto_trigger_incident: None,
            runbook_url: None,
            alert_type: None,
            service_name: None,
            window_secs: None,
            baseline_offset_secs: None,
            threshold_percent: None,
        },
    )
    .await
    .unwrap();
    silence_alert_rule(&pool, tenant, silenced.rule_id, true)
        .await
        .unwrap();

    let rules = list_alert_rules(&pool, tenant).await.unwrap();

    let state_for = |name: &str| {
        rules
            .iter()
            .find(|rule| rule.name == name)
            .map(|rule| rule.state.as_str())
            .unwrap()
    };
    assert_eq!(state_for("Pending rule"), "pending");
    assert_eq!(state_for("Active rule"), "active");
    assert_eq!(state_for("Resolved rule"), "resolved");
    assert_eq!(state_for("Silenced rule"), "silenced");
}

#[tokio::test]
async fn create_deadman_rule_appears_in_list_with_no_data_operator() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Checkout silent".into(),
        metric_name: String::new(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("deadman".into()),
        service_name: Some("checkout".into()),
        window_secs: Some(300),
        baseline_offset_secs: None,
        threshold_percent: None,
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.metric_name, "checkout");
    assert_eq!(created.operator, "no_data");
    assert!((created.threshold - 300.0).abs() < f64::EPSILON);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules
            .iter()
            .any(|r| r.rule_id == created.rule_id && r.operator == "no_data"),
        "created deadman rule must appear in list with no_data operator"
    );
}

#[tokio::test]
async fn create_deadman_rule_rejects_blank_service_name() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Checkout silent".into(),
        metric_name: String::new(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("deadman".into()),
        service_name: Some("   ".into()),
        window_secs: Some(300),
        baseline_offset_secs: None,
        threshold_percent: None,
    };
    let err = create_alert_rule(&pool, tenant, &req).await.unwrap_err();
    assert!(matches!(err, CreateRuleError::InvalidInput(_)));
}

#[tokio::test]
async fn create_change_detection_rule_appears_in_list_with_change_detection_operator() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Error rate change".into(),
        metric_name: "error_rate".into(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("change_detection".into()),
        service_name: None,
        window_secs: Some(300),
        baseline_offset_secs: Some(86400),
        threshold_percent: Some(50.0),
    };
    let created = create_alert_rule(&pool, tenant, &req).await.unwrap();

    assert_eq!(created.metric_name, "error_rate");
    assert_eq!(created.operator, "change_detection");
    assert!((created.threshold - 50.0).abs() < f64::EPSILON);

    let rules = list_alert_rules(&pool, tenant).await.unwrap();
    assert!(
        rules
            .iter()
            .any(|r| r.rule_id == created.rule_id && r.operator == "change_detection"),
        "created change_detection rule must appear in list with change_detection operator"
    );
}

#[tokio::test]
async fn create_change_detection_rule_rejects_missing_threshold_percent() {
    let pool = test_support::postgres::shared_pool().await;
    let tenant = Uuid::new_v4();

    let req = CreateRuleRequest {
        name: "Error rate change".into(),
        metric_name: "error_rate".into(),
        operator: String::new(),
        threshold: 0.0,
        notification_channels: None,
        auto_trigger_incident: None,
        runbook_url: None,
        alert_type: Some("change_detection".into()),
        service_name: None,
        window_secs: Some(300),
        baseline_offset_secs: Some(86400),
        threshold_percent: None,
    };
    let err = create_alert_rule(&pool, tenant, &req).await.unwrap_err();
    assert!(matches!(err, CreateRuleError::InvalidInput(_)));
}
