use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clickhouse::Client;
use tracing::{info, warn};

pub struct RetentionConfig {
    pub hot_trace_days: u64,
    pub check_interval: Duration,
}

impl RetentionConfig {
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("TRACE_HOT_RETENTION_DAYS").ok(),
            std::env::var("RETENTION_CHECK_INTERVAL_SECONDS").ok(),
        )
    }

    fn from_values(retention_days: Option<String>, interval_secs: Option<String>) -> Self {
        let days = retention_days
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(14)
            .clamp(3, 14);
        let interval_secs = interval_secs
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(3600);
        Self {
            hot_trace_days: days,
            check_interval: Duration::from_secs(interval_secs),
        }
    }
}

/// Compute the cutoff timestamp in Unix nanoseconds.
/// Spans with `start_time_unix_nano` older than this value are outside the hot retention window.
pub fn cutoff_unix_nano(now_unix_secs: u64, retention_days: u64) -> u64 {
    now_unix_secs
        .saturating_sub(retention_days * 86_400)
        .saturating_mul(1_000_000_000)
}

pub async fn run_retention_cycle(ch: &Client, config: &RetentionConfig) -> anyhow::Result<()> {
    let now_secs = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let cutoff_ns = cutoff_unix_nano(now_secs, config.hot_trace_days);
    info!(
        retention_days = config.hot_trace_days,
        cutoff_unix_nano = cutoff_ns,
        "retention: issuing hot-tier span deletion"
    );
    let sql =
        format!("ALTER TABLE observable.spans DELETE WHERE start_time_unix_nano < {cutoff_ns}");
    ch.query(&sql).execute().await?;
    info!("retention: hot-tier span deletion mutation submitted");
    Ok(())
}

pub async fn start_retention_worker(ch: Client, config: RetentionConfig) {
    info!(
        hot_trace_days = config.hot_trace_days,
        check_interval_secs = config.check_interval.as_secs(),
        "retention: starting hot-tier retention worker"
    );
    let mut interval = tokio::time::interval(config.check_interval);
    loop {
        interval.tick().await;
        if let Err(e) = run_retention_cycle(&ch, &config).await {
            warn!(error = %e, "retention: span deletion cycle failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cutoff_is_retention_days_before_now() {
        let now_secs = 1_746_000_000u64;
        let days = 14u64;
        let cutoff = cutoff_unix_nano(now_secs, days);
        let expected = (now_secs - days * 86_400) * 1_000_000_000;
        assert_eq!(cutoff, expected);
    }

    #[test]
    fn cutoff_does_not_underflow() {
        let cutoff = cutoff_unix_nano(0, 14);
        assert_eq!(cutoff, 0);
    }

    #[test]
    fn config_defaults_when_env_unset() {
        let config = RetentionConfig::from_values(None, None);
        assert_eq!(config.hot_trace_days, 14);
        assert_eq!(config.check_interval, Duration::from_secs(3600));
    }

    #[test]
    fn config_clamps_below_minimum() {
        let config = RetentionConfig::from_values(Some("1".into()), None);
        assert_eq!(config.hot_trace_days, 3);
    }

    #[test]
    fn config_clamps_above_maximum() {
        let config = RetentionConfig::from_values(Some("30".into()), None);
        assert_eq!(config.hot_trace_days, 14);
    }

    #[test]
    fn cutoff_sql_contains_numeric_value() {
        let now_secs = 1_746_000_000u64;
        let days = 14u64;
        let cutoff_ns = cutoff_unix_nano(now_secs, days);
        let sql =
            format!("ALTER TABLE observable.spans DELETE WHERE start_time_unix_nano < {cutoff_ns}");
        assert!(sql.contains(&cutoff_ns.to_string()));
        assert!(sql.contains("observable.spans"));
        assert!(sql.contains("start_time_unix_nano"));
    }
}
