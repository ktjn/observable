import { useQuery } from "@tanstack/react-query";
import { getTenantUsageReport } from "../../api/usage";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { LoadingState } from "../../components/ui/loading-state";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTimeDisplay, type TimeFormat } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

function formatInterval(fromMs: number, toMs: number, format: TimeFormat): string {
  return `${formatTimestamp(fromMs * 1_000_000, format)} to ${formatTimestamp(toMs * 1_000_000, format)}`;
}

function countTone(value: number): "good" | "warn" | "bad" | "info" {
  if (value === 0) return "good";
  if (value > 1000) return "bad";
  if (value > 100) return "warn";
  return "info";
}

export function BillingReportPage() {
  const { tenantId, tenantName } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();

  const { data, isLoading } = useQuery({
    queryKey: ["tenant-usage-report", tenantId, fromMs, toMs],
    queryFn: () => getTenantUsageReport(tenantId, { from: fromMs, to: toMs }),
  });

  if (isLoading || !data) {
    return <LoadingState>Loading tenant usage report...</LoadingState>;
  }

  const telemetry = data.telemetry_summary;
  const control = data.control_plane_summary;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Operations</div>
          <h1>Tenant usage and cost report</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Billing interval: {formatInterval(fromMs, toMs, format)}. This report is a relative usage
            index, not an invoice.
          </p>
        </div>
      </div>

      <Panel title="Usage summary" eyebrow={`Tenant: ${tenantName}`}>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4" role="group" aria-label="Usage summary">
          <MetricCard label="Cost index" value={data.estimated_cost_index} tone={countTone(data.estimated_cost_index)} />
          <MetricCard label="Query reads" value={control.query_reads} tone={countTone(control.query_reads)} />
          <MetricCard label="Credential checks" value={control.credential_checks} tone={countTone(control.credential_checks)} />
          <MetricCard label="Metric series" value={telemetry.metric_series_created} tone={countTone(telemetry.metric_series_created)} />
        </div>
      </Panel>

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title="Telemetry volume" eyebrow="Hot-path data" className="h-full">
          <div
            className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            role="group"
            aria-label="Telemetry volume"
          >
            <MetricCard label="Spans" value={telemetry.spans} tone={countTone(telemetry.spans)} />
            <MetricCard label="Logs" value={telemetry.logs} tone={countTone(telemetry.logs)} />
            <MetricCard label="Metric points" value={telemetry.metric_points} tone={countTone(telemetry.metric_points)} />
            <MetricCard
              label="Metric series created"
              value={telemetry.metric_series_created}
              tone={countTone(telemetry.metric_series_created)}
            />
          </div>
        </Panel>

        <Panel title="Control-plane activity" eyebrow="Tenant work" className="h-full">
          <div
            className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            role="group"
            aria-label="Control-plane activity"
          >
            <MetricCard label="Query reads" value={control.query_reads} tone={countTone(control.query_reads)} />
            <MetricCard label="Query rows" value={control.query_rows} tone={countTone(control.query_rows)} />
            <MetricCard
              label="Credential checks"
              value={control.credential_checks}
              tone={countTone(control.credential_checks)}
            />
            <MetricCard
              label="Credential allowances"
              value={control.credential_allows}
              tone={control.credential_allows > 0 ? "good" : "info"}
            />
            <MetricCard
              label="Credential denials"
              value={control.credential_denies}
              tone={control.credential_denies > 0 ? "warn" : "good"}
            />
          </div>
        </Panel>
      </div>
    </section>
  );
}
