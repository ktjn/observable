//! Per-tenant ingest watermark (phase 2 E5, OBS-29-03 input): the latest observation the
//! store holds for one tenant, per signal.
//!
//! The store keeps no insertion time, so the watermark is the newest observation time
//! (span end, log observed time, metric point time) at or before `now` plus a clock-skew
//! allowance; future-dated rows cannot make ingestion look current. Every read is bound to
//! one tenant, so no caller can read another tenant's watermark through it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Future-dated observations beyond this allowance are ignored (nanoseconds).
pub const SKEW_ALLOWANCE_NS: u64 = 300 * 1_000_000_000;

/// Latest observation per signal in Unix nanoseconds, within the read window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestWatermark {
    pub traces: Option<u64>,
    pub logs: Option<u64>,
    pub metrics: Option<u64>,
}

/// Whether ingestion is keeping up for a tenant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatermarkStatus {
    /// Some signal is within the bound.
    Current,
    /// Data exists in the window, but none within the bound.
    Delayed,
    /// No data in the window.
    Missing,
}

impl IngestWatermark {
    pub fn latest(&self) -> Option<u64> {
        self.traces.max(self.logs).max(self.metrics)
    }

    pub fn status(&self, now_ns: u64, bound_ns: u64) -> WatermarkStatus {
        match self.latest() {
            None => WatermarkStatus::Missing,
            Some(t) if now_ns.saturating_sub(t) <= bound_ns => WatermarkStatus::Current,
            Some(_) => WatermarkStatus::Delayed,
        }
    }
}

#[derive(clickhouse::Row, Deserialize)]
struct SignalRow {
    signal: String,
    latest: u64,
}

/// Reads the watermark of `tenant_id` from observations in `[since_ns, now_ns + skew]`.
pub async fn ingest_watermark(
    ch: &clickhouse::Client,
    tenant_id: Uuid,
    since_ns: u64,
    now_ns: u64,
) -> Result<IngestWatermark, clickhouse::error::Error> {
    let until = now_ns.saturating_add(SKEW_ALLOWANCE_NS);
    let rows: Vec<SignalRow> = ch
        .query(
            "SELECT 'traces' AS signal, max(end_time_unix_nano) AS latest FROM observable.spans \
             WHERE tenant_id = ? AND end_time_unix_nano >= ? AND end_time_unix_nano <= ? \
             UNION ALL SELECT 'logs', max(observed_timestamp_unix_nano) FROM observable.logs \
             WHERE tenant_id = ? AND observed_timestamp_unix_nano >= ? AND observed_timestamp_unix_nano <= ? \
             UNION ALL SELECT 'metrics', max(time_unix_nano) FROM observable.metric_points \
             WHERE tenant_id = ? AND time_unix_nano >= ? AND time_unix_nano <= ?",
        )
        .bind(tenant_id)
        .bind(since_ns)
        .bind(until)
        .bind(tenant_id)
        .bind(since_ns)
        .bind(until)
        .bind(tenant_id)
        .bind(since_ns)
        .bind(until)
        .fetch_all()
        .await?;
    let mut out = IngestWatermark::default();
    for r in rows {
        let v = (r.latest > 0).then_some(r.latest);
        match r.signal.as_str() {
            "traces" => out.traces = v,
            "logs" => out.logs = v,
            "metrics" => out.metrics = v,
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_follows_the_newest_signal() {
        let s = 1_000_000_000u64;
        let w = IngestWatermark {
            traces: Some(100 * s),
            logs: Some(390 * s),
            metrics: None,
        };
        assert_eq!(w.latest(), Some(390 * s));
        assert_eq!(w.status(400 * s, 300 * s), WatermarkStatus::Current);
        assert_eq!(w.status(800 * s, 300 * s), WatermarkStatus::Delayed);
        assert_eq!(
            IngestWatermark::default().status(800 * s, 300 * s),
            WatermarkStatus::Missing
        );
    }
}
