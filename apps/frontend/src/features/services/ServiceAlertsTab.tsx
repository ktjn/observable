import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listAlertRules, type AlertRuleItem } from "../../api/alerts";
import { listIncidents, type IncidentItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

function severityTone(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical":
      return "bad";
    case "warning":
      return "warn";
    default:
      return "neutral";
  }
}

function statusTone(status: string): "bad" | "warn" | "good" | "neutral" {
  switch (status) {
    case "triggered":
      return "bad";
    case "acknowledged":
      return "warn";
    case "resolved":
      return "good";
    default:
      return "neutral";
  }
}

export function ServiceAlertsTab() {
  const { tenantId } = useTenantContext();
  const { format } = useTimeDisplay();

  const { data: rulesData } = useQuery({
    queryKey: ["alert-rules", tenantId],
    queryFn: () => listAlertRules(tenantId),
  });

  const { data: incidentsData } = useQuery({
    queryKey: ["incidents", tenantId],
    queryFn: () => listIncidents(tenantId),
  });

  const firingRules = (rulesData?.items ?? []).filter(
    (r: AlertRuleItem) => r.firing || r.state === "active",
  );
  const openIncidents = (incidentsData?.items ?? []).filter(
    (i: IncidentItem) => i.status !== "resolved",
  );

  return (
    <div className="space-y-6 p-4">
      <section aria-label="Firing alert rules">
        <h3 className="mb-3 text-sm font-semibold">Firing Alert Rules</h3>
        {firingRules.length === 0 ? (
          <p className="text-sm text-[var(--muted)]">No firing rules.</p>
        ) : (
          <table className="w-full text-sm" aria-label="Firing alert rules table">
            <thead>
              <tr className="border-b text-left">
                <th className="pb-2 pr-4">Name</th>
                <th className="pb-2 pr-4">Metric</th>
                <th className="pb-2 pr-4">Severity</th>
                <th className="pb-2 pr-4">Last Fired</th>
              </tr>
            </thead>
            <tbody>
              {firingRules.map((rule) => (
                <tr key={rule.rule_id} className="modern-table-row border-l-2 border-l-[var(--bad)]">
                  <td className="py-2 pr-4 font-medium">{rule.name}</td>
                  <td className="py-2 pr-4 font-mono text-xs text-[var(--muted)]">
                    {rule.metric_name}
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={severityTone(rule.severity)}>{rule.severity}</Badge>
                  </td>
                  <td className="py-2 pr-4 text-[var(--muted)]">
                    {rule.last_fired_at
                      ? formatTimestamp(
                          new Date(rule.last_fired_at).getTime() * 1_000_000,
                          format,
                        )
                      : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section aria-label="Open incidents">
        <h3 className="mb-3 text-sm font-semibold">Open Incidents</h3>
        {openIncidents.length === 0 ? (
          <p className="text-sm text-[var(--muted)]">No open incidents.</p>
        ) : (
          <table className="w-full text-sm" aria-label="Open incidents table">
            <thead>
              <tr className="border-b text-left">
                <th className="pb-2 pr-4">Title</th>
                <th className="pb-2 pr-4">Severity</th>
                <th className="pb-2 pr-4">Status</th>
                <th className="pb-2 pr-4">Triggered</th>
              </tr>
            </thead>
            <tbody>
              {openIncidents.map((incident) => (
                <tr
                  key={incident.incident_id}
                  className="modern-table-row border-l-2 border-l-[var(--bad)]"
                >
                  <td className="py-2 pr-4">
                    <Link
                      to="/incidents/$incidentId"
                      params={{ incidentId: incident.incident_id }}
                      className="font-medium hover:underline"
                    >
                      {incident.title}
                    </Link>
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={severityTone(incident.severity)}>{incident.severity}</Badge>
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={statusTone(incident.status)}>{incident.status}</Badge>
                  </td>
                  <td className="py-2 pr-4 text-[var(--muted)]">
                    {formatTimestamp(
                      new Date(incident.triggered_at).getTime() * 1_000_000,
                      format,
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}
