import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listIncidents, type IncidentItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { CopyButton } from "../../components/ui/copy-button";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

type StatusFilter = "" | "triggered" | "acknowledged" | "resolved";

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical": return "bad";
    case "warning":  return "warn";
    default:         return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":    return "bad";
    case "acknowledged": return "warn";
    case "resolved":     return "good";
    default:             return "neutral";
  }
}

export function IncidentsPage() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("");

  const { data, isLoading } = useQuery({
    queryKey: ["incidents", tenantId],
    queryFn: () => listIncidents(tenantId),
  });

  const items = data?.items ?? [];
  const triggeredCount    = items.filter((i) => i.status === "triggered").length;
  const acknowledgedCount = items.filter((i) => i.status === "acknowledged").length;
  const resolvedCount     = items.filter((i) => i.status === "resolved").length;

  const resolvedItems = items.filter((i) => i.status === "resolved" && i.resolved_at != null);
  const mttrMin = resolvedItems.length
    ? Math.round(
        resolvedItems.reduce(
          (sum, i) =>
            sum + (new Date(i.resolved_at!).getTime() - new Date(i.triggered_at).getTime()),
          0,
        ) /
          resolvedItems.length /
          60_000,
      )
    : null;

  const filteredItems = statusFilter ? items.filter((i) => i.status === statusFilter) : items;

  const pillDefs: { label: string; value: StatusFilter; count: number; activeColor: string }[] = [
    { label: "All",            value: "",             count: items.length,     activeColor: "var(--brand)" },
    { label: "Triggered",      value: "triggered",    count: triggeredCount,   activeColor: "var(--bad)"   },
    { label: "Acknowledged",   value: "acknowledged", count: acknowledgedCount, activeColor: "var(--warn)" },
    { label: "Resolved",       value: "resolved",     count: resolvedCount,    activeColor: "var(--good)"  },
  ];

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Reliability</div>
          <h1>Incidents</h1>
        </div>
      </div>

      <div
        className="grid grid-cols-1 gap-4 sm:grid-cols-5"
        aria-label="Incident summary"
        role="group"
      >
        <MetricCard label="Total"        value={items.length}                          tone="info"                                     />
        <MetricCard label="Triggered"    value={triggeredCount}                        tone={triggeredCount > 0 ? "bad" : "good"}      />
        <MetricCard label="Acknowledged" value={acknowledgedCount}                     tone={acknowledgedCount > 0 ? "warn" : "info"}  />
        <MetricCard label="Resolved"     value={resolvedCount}                         tone="good"                                     />
        <MetricCard label="MTTR"         value={mttrMin !== null ? `${mttrMin}m` : "—"} tone="info"                                   />
      </div>

      <div className="flex items-center gap-1" role="group" aria-label="Filter incidents">
        {pillDefs.map(({ label, value, count, activeColor }) => {
          const isActive = statusFilter === value;
          return (
            <button
              key={value || "all"}
              type="button"
              onClick={() => setStatusFilter(value)}
              className={[
                "flex items-center gap-1.5 px-2.5 py-1 text-xs font-bold border rounded transition-colors",
                isActive
                  ? ""
                  : "border-[var(--border)] text-[var(--muted)] hover:border-[var(--brand)] hover:text-[var(--text)]",
              ].join(" ")}
              style={isActive ? { borderColor: activeColor, color: activeColor } : undefined}
            >
              <span>{label}</span>
              <span aria-hidden="true">({count})</span>
            </button>
          );
        })}
      </div>

      <Panel eyebrow="Active and historical">
        {isLoading ? (
          <LoadingState>Loading incidents...</LoadingState>
        ) : !filteredItems.length ? (
          <EmptyState title="No incidents found." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm" aria-label="Incidents list">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 pr-4">Title</th>
                  <th className="pb-2 pr-4">Severity</th>
                  <th className="pb-2 pr-4">Status</th>
                  <th className="pb-2 pr-4">Triggered At</th>
                  <th className="pb-2 pr-4">Resolved At</th>
                </tr>
              </thead>
              <tbody>
                {filteredItems.map((incident: IncidentItem) => {
                  const rowClass = [
                    "modern-table-row border-l-2",
                    incident.status === "triggered"
                      ? "border-l-[var(--bad)]"
                      : incident.status === "acknowledged"
                        ? "border-l-[var(--warn)]"
                        : "border-l-transparent",
                  ].join(" ");
                  return (
                    <tr key={incident.incident_id} className={rowClass}>
                      <td className="py-2 pr-4 group">
                        <span className="inline-flex items-center gap-1">
                          <Link
                            to="/incidents/$incidentId"
                            params={{ incidentId: incident.incident_id }}
                            className="font-medium hover:underline"
                          >
                            {incident.title}
                          </Link>
                          <CopyButton value={incident.title} label="Copy incident title" />
                        </span>
                      </td>
                      <td className="py-2 pr-4">
                        <Badge tone={severityColor(incident.severity)}>{incident.severity}</Badge>
                      </td>
                      <td className="py-2 pr-4">
                        <Badge tone={statusColor(incident.status)}>{incident.status}</Badge>
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {formatTimestamp(isoToNs(incident.triggered_at), format)}
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {incident.resolved_at
                          ? formatTimestamp(isoToNs(incident.resolved_at), format)
                          : "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </section>
  );
}
