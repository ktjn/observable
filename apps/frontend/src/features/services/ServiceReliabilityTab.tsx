import { useQuery } from "@tanstack/react-query";
import { getServiceReliabilityReport } from "../../api/reliability";
import { Badge } from "../../components/ui/badge";
import { CopyableText } from "../../components/ui/copy-button";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp, isoToNs } from "../../utils/formatTimestamp";

type DeploymentTone = "good" | "warn" | "bad" | "info";

function deploymentTone(status: string): DeploymentTone {
  switch (status) {
    case "success":
      return "good";
    case "failed":
    case "rolled_back":
      return "bad";
    case "in_progress":
      return "warn";
    default:
      return "info";
  }
}

function formatMinutes(value: number | null): string {
  if (value == null) return "—";
  if (value < 10) return `${value.toFixed(1)}m`;
  return `${Math.round(value)}m`;
}

export function ServiceReliabilityTab({ serviceName }: { serviceName: string }) {
  const { tenantId, environment } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();

  const { data, isLoading } = useQuery({
    queryKey: ["service-reliability-report", tenantId, serviceName, environment, fromMs, toMs],
    queryFn: () =>
      getServiceReliabilityReport(tenantId, serviceName, {
        environment: environment ?? undefined,
        from: fromMs,
        to: toMs,
      }),
  });

  if (isLoading) {
    return <LoadingState>Loading reliability report...</LoadingState>;
  }

  if (!data) {
    return <EmptyState title="Reliability report unavailable." />;
  }

  const incidents = data.incidents ?? [];
  const slos = data.slos ?? [];
  const deployments = data.deployments ?? [];
  const latestDeployment = deployments[0] ?? null;

  const hasData =
    incidents.length > 0 ||
    slos.length > 0 ||
    deployments.length > 0 ||
    data.incident_summary.total > 0 ||
    data.slo_summary.total > 0 ||
    data.deployment_summary.total > 0;

  if (!hasData) {
    return (
      <EmptyState
        title="No reliability data yet."
        description="This service has no incidents, SLOs, or deployments in the selected window."
      />
    );
  }

  return (
    <div className="space-y-6 p-4">
      <div className="space-y-1">
        <div className="text-xs font-bold uppercase text-[var(--muted)]">Reliability</div>
        <h3 className="text-lg font-semibold text-[var(--text-strong)]">
          {serviceName}
        </h3>
        <p className="text-sm text-[var(--muted)]">
          Window: {formatTimestamp(isoToNs(data.from), format)} to{" "}
          {formatTimestamp(isoToNs(data.to), format)}
        </p>
      </div>

      <div
        className="grid grid-cols-1 gap-4 sm:grid-cols-4"
        role="group"
        aria-label="Reliability summary"
      >
        <MetricCard label="Incidents" value={data.incident_summary.total} tone="info" />
        <MetricCard label="Open Incidents" value={data.incident_summary.open} tone={data.incident_summary.open > 0 ? "warn" : "good"} />
        <MetricCard label="Firing SLOs" value={data.slo_summary.firing} tone={data.slo_summary.firing > 0 ? "bad" : "good"} />
        <MetricCard label="Deployments" value={data.deployment_summary.total} tone={data.deployment_summary.total > 0 ? "info" : "good"} />
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title="Recent incidents" eyebrow="Incident load">
          <div className="mb-3 text-sm text-[var(--muted)]">
            Mean time to resolve: {formatMinutes(data.incident_summary.mean_time_to_resolve_minutes)}
          </div>
          {incidents.length === 0 ? (
            <EmptyState title="No incidents in this window." />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm" aria-label="Recent incidents">
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
                  {incidents.map((incident) => (
                    <tr key={incident.incident_id} className="modern-table-row">
                      <td className="py-2 pr-4 font-medium">{incident.title}</td>
                      <td className="py-2 pr-4">
                        <Badge tone={incident.severity === "critical" ? "bad" : incident.severity === "warning" ? "warn" : "info"}>
                          {incident.severity}
                        </Badge>
                      </td>
                      <td className="py-2 pr-4">
                        <Badge tone={incident.status === "resolved" ? "good" : incident.status === "triggered" ? "bad" : "warn"}>
                          {incident.status}
                        </Badge>
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {formatTimestamp(isoToNs(incident.triggered_at), format)}
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">
                        {incident.resolved_at ? formatTimestamp(isoToNs(incident.resolved_at), format) : "—"}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>

        <Panel title="SLO coverage" eyebrow="Burn-rate signals">
          {slos.length === 0 ? (
            <EmptyState title="No SLOs for this service." />
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm" aria-label="Service SLOs">
                <thead>
                  <tr className="border-b text-left">
                    <th className="pb-2 pr-4">Description</th>
                    <th className="pb-2 pr-4">Environment</th>
                    <th className="pb-2 pr-4">State</th>
                    <th className="pb-2 pr-4">Target</th>
                    <th className="pb-2 pr-4">Window</th>
                  </tr>
                </thead>
                <tbody>
                  {slos.map((slo) => (
                    <tr key={slo.slo_id} className="modern-table-row">
                      <td className="py-2 pr-4 font-medium">
                        {slo.description || `${slo.service_name} availability`}
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">{slo.environment}</td>
                      <td className="py-2 pr-4">
                        <Badge tone={slo.firing ? "bad" : "good"}>{slo.firing ? "Firing" : "Healthy"}</Badge>
                      </td>
                      <td className="py-2 pr-4 text-[var(--muted)]">{(slo.target * 100).toFixed(1)}%</td>
                      <td className="py-2 pr-4 text-[var(--muted)]">{slo.window_days}d</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Panel>
      </div>

      <Panel title="Deployments" eyebrow="Release context">
        {deployments.length === 0 ? (
          <EmptyState title="No deployments in this window." />
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-sm" aria-label="Service deployments">
              <thead>
                <tr className="border-b text-left">
                  <th className="pb-2 pr-4">Version</th>
                  <th className="pb-2 pr-4">Environment</th>
                  <th className="pb-2 pr-4">Status</th>
                  <th className="pb-2 pr-4">Started</th>
                  <th className="pb-2 pr-4">By</th>
                  <th className="pb-2 pr-4">Commit</th>
                </tr>
              </thead>
              <tbody>
                {deployments.map((deployment) => (
                  <tr key={deployment.deployment_id} className="modern-table-row">
                    <td className="py-2 pr-4 font-mono text-xs">
                      <CopyableText value={deployment.service_version} label="Copy version" mono />
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">{deployment.environment}</td>
                    <td className="py-2 pr-4">
                      <Badge tone={deploymentTone(deployment.status)}>{deployment.status}</Badge>
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">
                      {formatTimestamp(isoToNs(deployment.started_at), format)}
                    </td>
                    <td className="py-2 pr-4 text-[var(--muted)]">{deployment.deployed_by ?? "—"}</td>
                    <td className="py-2 pr-4 font-mono text-xs text-[var(--muted)]">
                      {deployment.commit_sha ? (
                        <CopyableText value={deployment.commit_sha} label="Copy commit SHA" mono>
                          {deployment.commit_sha.slice(0, 8)}
                        </CopyableText>
                      ) : (
                        "—"
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {latestDeployment && (
          <p className="mt-3 text-sm text-[var(--muted)]">
            Latest deployment: {latestDeployment.service_version} in {latestDeployment.environment}
          </p>
        )}
      </Panel>
    </div>
  );
}
