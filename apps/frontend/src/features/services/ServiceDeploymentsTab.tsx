import { useQuery } from "@tanstack/react-query";
import { listDeployments, type DeploymentMarker } from "../../api/deployments";
import { Badge } from "../../components/ui/badge";
import { CopyableText } from "../../components/ui/copy-button";
import { EmptyState } from "../../components/ui/empty-state";
import { LoadingState } from "../../components/ui/loading-state";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

function deploymentStatusTone(
  status: DeploymentMarker["status"],
): "good" | "bad" | "warn" | "info" {
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

export function ServiceDeploymentsTab({ serviceName }: { serviceName: string }) {
  const { tenantId } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();

  const { data, isLoading } = useQuery({
    queryKey: ["deployments", tenantId, serviceName, fromMs, toMs],
    queryFn: () =>
      listDeployments(tenantId, {
        service_name: serviceName,
        start_time: new Date(fromMs).toISOString(),
        end_time: new Date(toMs).toISOString(),
        limit: 50,
      }),
  });

  const items = data?.items ?? [];

  if (isLoading) {
    return <LoadingState>Loading deployments…</LoadingState>;
  }

  if (!items.length) {
    return <EmptyState title="No deployments found." />;
  }

  return (
    <div className="overflow-x-auto p-4">
      <table className="w-full text-sm" aria-label="Deployments">
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
          {items.map((dep) => (
            <tr key={dep.deployment_id} className="modern-table-row">
              <td className="py-2 pr-4 font-mono text-xs">
                <CopyableText value={dep.service_version} label="Copy version" mono />
              </td>
              <td className="py-2 pr-4">{dep.environment}</td>
              <td className="py-2 pr-4">
                <Badge tone={deploymentStatusTone(dep.status)}>{dep.status}</Badge>
              </td>
              <td className="py-2 pr-4 text-[var(--muted)]">
                {formatTimestamp(
                  new Date(dep.started_at).getTime() * 1_000_000,
                  format,
                )}
              </td>
              <td className="py-2 pr-4 text-[var(--muted)]">
                {dep.deployed_by ?? "—"}
              </td>
              <td className="py-2 pr-4 font-mono text-xs text-[var(--muted)]">
                {dep.commit_sha ? (
                  <CopyableText value={dep.commit_sha} label="Copy commit SHA" mono>
                    {dep.commit_sha.slice(0, 8)}
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
  );
}
