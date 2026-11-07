//! Kafka-compatible broker selection for integration tests.

/// Bootstrap servers of an external broker (e.g. the Redpanda started by
/// `scripts/local-stack.sh`). Tests that only need topic isolation use it
/// instead of starting a container; tests that stop or pause the broker
/// must keep a dedicated container.
pub const EXTERNAL_BROKERS_ENV: &str = "OBSERVABLE_TEST_KAFKA_BROKERS";

/// Returns the external broker list when configured and non-empty.
pub fn external_brokers() -> Option<String> {
    std::env::var(EXTERNAL_BROKERS_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}
