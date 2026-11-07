//! Contract suites for `decide-access` and `lookup-principals` in mock mode. The same suites
//! run against the real provider in services/auth-service/tests/authz_contract_integration.rs.

use observable_auth::contract::{self, MockDecideHarness, MockPrincipalsHarness};

#[tokio::test]
async fn mock_permit_is_recorded_with_actor_purpose_and_elevation() {
    contract::permit_is_recorded_with_actor_purpose_and_elevation(&MockDecideHarness::default())
        .await;
}

#[tokio::test]
async fn mock_denied_and_nonexistent_are_indistinguishable() {
    contract::denied_and_nonexistent_are_indistinguishable(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_purpose_missing_is_denied_and_recorded() {
    contract::purpose_missing_is_denied_and_recorded(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_expired_elevation_is_denied() {
    contract::expired_elevation_is_denied(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_unavailable_store_denies_every_request() {
    contract::unavailable_store_denies_every_request(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_filter_returns_only_permitted_resources() {
    contract::filter_returns_only_permitted_resources(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_incomplete_request_is_denied() {
    contract::incomplete_request_is_denied(&MockDecideHarness::default()).await;
}

#[tokio::test]
async fn mock_lookup_principals_suite() {
    contract::run_lookup_principals_suite(&MockPrincipalsHarness::default()).await;
}

#[tokio::test]
async fn mock_delay_scenario_still_decides() {
    use observable_auth::authz::{AccessDecider, MockAccessDecider, MockScenario};
    use std::time::{Duration, Instant};
    let m = MockAccessDecider::new();
    m.set_scenario(MockScenario::Delay(Duration::from_millis(50)));
    let start = Instant::now();
    let d = m.decide(&Default::default()).await;
    assert!(start.elapsed() >= Duration::from_millis(50));
    assert_eq!(d, Ok(observable_auth::authz::Decision::Deny));
}
