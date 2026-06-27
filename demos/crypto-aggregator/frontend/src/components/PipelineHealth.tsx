import { useEffect, useState } from "react";
import type { PipelineMetrics } from "../generated/pipeline.PipelineMetrics.v1";

const POLL_MS = 2_000;

type ObsStatus = PipelineMetrics["observable_status"];

const OBS_STATUS_COLOR: Record<ObsStatus, string> = {
  Ok: "text-positive",
  Degraded: "text-yellow-400",
  Offline: "text-negative",
};

const OBS_STATUS_DOT: Record<ObsStatus, string> = {
  Ok: "bg-positive animate-pulse",
  Degraded: "bg-yellow-400",
  Offline: "bg-negative",
};

const OBS_STATUS_LABEL: Record<ObsStatus, string> = {
  Ok: "Connected",
  Degraded: "Degraded",
  Offline: "Offline",
};

function Gauge({ label, value, unit, warn = 0.7, danger = 0.9 }: {
  label: string;
  value: number;
  unit: string;
  warn?: number;
  danger?: number;
}) {
  const color =
    value >= danger
      ? "text-negative"
      : value >= warn
        ? "text-yellow-400"
        : "text-positive";
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-xs text-muted">{label}</span>
      <span className={`font-mono text-lg font-semibold ${color}`}>
        {value.toLocaleString(undefined, { maximumFractionDigits: 1 })}
        <span className="ml-1 text-xs font-normal text-muted">{unit}</span>
      </span>
    </div>
  );
}

export function PipelineHealth() {
  const [metrics, setMetrics] = useState<PipelineMetrics | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    let timeout: ReturnType<typeof setTimeout>;

    const poll = async () => {
      try {
        const res = await fetch("/metrics");
        if (!res.ok) throw new Error("non-200");
        const data = (await res.json()) as PipelineMetrics;
        setMetrics(data);
        setError(false);
      } catch {
        setError(true);
      }
      timeout = setTimeout(poll, POLL_MS);
    };

    void poll();
    return () => clearTimeout(timeout);
  }, []);

  const obsStatus: ObsStatus = metrics?.observable_status ?? "Offline";

  return (
    <div data-testid="pipeline-health" className="space-y-3">
      {error && (
        <p className="text-xs text-negative">Backend unreachable — retrying…</p>
      )}
      {metrics == null && !error && (
        <p className="text-xs text-muted animate-pulse">Loading pipeline metrics…</p>
      )}
      {metrics && (
        <div className="grid grid-cols-2 gap-x-6 gap-y-4 sm:grid-cols-3">
          <Gauge label="Ingest Rate" value={metrics.ingest_rate} unit="ev/s" />
          <Gauge
            label="Correlation Lag"
            value={metrics.correlation_lag_ms}
            unit="ms"
            warn={1000}
            danger={3000}
          />
          <Gauge
            label="Buffer Fill"
            value={Math.round(metrics.buffer_fill_ratio * 100)}
            unit="%"
            warn={70}
            danger={90}
          />
          <Gauge
            label="Exporter Latency"
            value={metrics.exporter_latency_ms}
            unit="ms"
            warn={500}
            danger={2000}
          />
          <Gauge
            label="Errors"
            value={metrics.error_count}
            unit="total"
            warn={10}
            danger={50}
          />
        </div>
      )}

      {/* Observable ingestion connection status */}
      <div className="pt-2 border-t border-slate-800 flex items-center gap-2">
        <span className={`inline-block size-2 rounded-full flex-shrink-0 ${OBS_STATUS_DOT[obsStatus]}`} />
        <span className="text-xs text-muted">Observable</span>
        <span className={`text-xs font-medium ${OBS_STATUS_COLOR[obsStatus]}`}>
          {OBS_STATUS_LABEL[obsStatus]}
        </span>
      </div>
    </div>
  );
}
