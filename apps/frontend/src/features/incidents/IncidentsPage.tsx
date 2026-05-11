import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { listIncidents, type IncidentItem } from "../../api/incidents";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { Toolbar } from "../../components/ui/toolbar";
import { Tabs } from "../../components/ui/tabs";
import { useTenantContext } from "../../hooks/useTenantContext";

const statusTabs = [
  { label: "All", value: "" },
  { label: "Triggered", value: "triggered" },
  { label: "Acknowledged", value: "acknowledged" },
  { label: "Resolved", value: "resolved" },
];

function severityColor(severity: string): "bad" | "warn" | "neutral" {
  switch (severity) {
    case "critical":
      return "bad";
    case "warning":
      return "warn";
    default:
      return "neutral";
  }
}

function statusColor(status: string): "bad" | "warn" | "good" | "neutral" {
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

export function IncidentsPage() {
  const { tenantId } = useTenantContext();
  const [activeTab, setActiveTab] = useState("");

  const { data, isLoading } = useQuery({
    queryKey: ["incidents", tenantId, activeTab],
    queryFn: () => listIncidents(tenantId, activeTab || undefined),
  });

  return (
    <div className="space-y-4">
      <Toolbar>Incidents</Toolbar>

      <Tabs.Root value={activeTab} onValueChange={setActiveTab}>
        <Tabs.List>
          {statusTabs.map((tab) => (
            <Tabs.Tab key={tab.value} value={tab.value}>
              {tab.label}
            </Tabs.Tab>
          ))}
        </Tabs.List>
      </Tabs.Root>

      <Panel>
        {isLoading ? (
          <LoadingState>Loading incidents...</LoadingState>
        ) : !data?.items.length ? (
          <EmptyState title="No incidents found." />
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b text-left">
                <th className="pb-2 pr-4">Title</th>
                <th className="pb-2 pr-4">Severity</th>
                <th className="pb-2 pr-4">Status</th>
                <th className="pb-2 pr-4">Triggered</th>
                <th className="pb-2 pr-4">Resolved</th>
              </tr>
            </thead>
            <tbody>
              {data.items.map((incident: IncidentItem) => (
                <tr key={incident.incident_id} className="border-b hover:bg-muted/50">
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
                    <Badge tone={severityColor(incident.severity)}>
                      {incident.severity}
                    </Badge>
                  </td>
                  <td className="py-2 pr-4">
                    <Badge tone={statusColor(incident.status)}>
                      {incident.status}
                    </Badge>
                  </td>
                  <td className="py-2 pr-4 text-muted-foreground">
                    {new Date(incident.triggered_at).toLocaleString()}
                  </td>
                  <td className="py-2 pr-4 text-muted-foreground">
                    {incident.resolved_at
                      ? new Date(incident.resolved_at).toLocaleString()
                      : "—"}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>
    </div>
  );
}
