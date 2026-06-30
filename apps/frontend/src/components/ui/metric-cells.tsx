// Threshold: ≥5% = bad, ≥1% = warn, <1% = good
export function ErrorRateCell({ value }: { value: number }) {
  // value is a fraction (0.05 = 5%)
  const pct = value * 100;
  const color = pct >= 5 ? "var(--bad)" : pct >= 1 ? "var(--warn)" : "var(--good)";
  return (
    <span className="tabular-nums" style={{ color }}>
      {pct.toFixed(2)}%
    </span>
  );
}

// Threshold: ≥500ms = bad, ≥100ms = warn, <100ms = good
export function LatencyCell({ valueMs }: { valueMs: number }) {
  const color = valueMs >= 500 ? "var(--bad)" : valueMs >= 100 ? "var(--warn)" : "var(--good)";
  return (
    <span className="tabular-nums" style={{ color }}>
      {Math.round(valueMs)}ms
    </span>
  );
}

// Same thresholds as LatencyCell but input is nanoseconds
export function DurationCell({ durationNs }: { durationNs: number }) {
  const ms = durationNs / 1_000_000;
  const color = ms >= 500 ? "var(--bad)" : ms >= 100 ? "var(--warn)" : "var(--good)";
  return (
    <span className="tabular-nums" style={{ color }}>
      {ms.toFixed(2)}ms
    </span>
  );
}
