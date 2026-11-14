import { useEffect, useState } from "react";

export type ViewFreshnessMode = "live" | "historical" | "scheduled_report";

interface DataFreshnessProps {
  dataUpdatedAt: number; // Unix ms from React Query's dataUpdatedAt
  viewMode?: ViewFreshnessMode;
  scheduleLabel?: string;
  autoRefresh?: boolean;
  hasPendingUpdate?: boolean;
  onApplyPendingUpdate?: () => void;
}

const MODE_LABELS: Record<ViewFreshnessMode, string> = {
  live: "Live operational",
  historical: "Historical analysis",
  scheduled_report: "Scheduled report",
};

export function DataFreshness({
  dataUpdatedAt,
  viewMode = "live",
  scheduleLabel,
  autoRefresh,
  hasPendingUpdate = false,
  onApplyPendingUpdate,
}: DataFreshnessProps) {
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    setNow(Date.now());
  }, [dataUpdatedAt]);

  if (dataUpdatedAt === 0 && viewMode === "live") return null;

  const diffSec = Math.max(0, Math.floor((now - dataUpdatedAt) / 1000));
  let label: string;
  if (diffSec < 60) {
    label = `${diffSec}s ago`;
  } else if (diffSec < 3600) {
    label = `${Math.floor(diffSec / 60)}m ago`;
  } else {
    label = `${Math.floor(diffSec / 3600)}h ago`;
  }

  return (
    <span className="inline-flex items-center gap-1.5 text-xs text-[var(--muted)]" data-testid="data-freshness">
      <span
        data-testid="view-freshness-mode"
        className="rounded border border-[var(--border)] px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-[var(--text)]"
      >
        {MODE_LABELS[viewMode]}
        {viewMode === "scheduled_report" && scheduleLabel ? ` (${scheduleLabel})` : ""}
      </span>
      {dataUpdatedAt > 0 && <span>Updated {label}</span>}
      {typeof autoRefresh === "boolean" && (
        <span
          data-testid="auto-refresh-indicator"
          className="rounded border border-[var(--border)] px-1.5 py-0.5 text-[10px]"
        >
          {autoRefresh ? "Auto-refresh on" : "Auto-refresh paused"}
        </span>
      )}
      {hasPendingUpdate && (
        <span role="status" data-testid="pending-update-notice" className="inline-flex items-center gap-1">
          <span>New data available</span>
          {onApplyPendingUpdate && (
            <button
              type="button"
              onClick={onApplyPendingUpdate}
              className="font-semibold text-[var(--brand)] underline"
            >
              Apply update
            </button>
          )}
        </span>
      )}
    </span>
  );
}
