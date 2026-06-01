import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { listEnvironments, listTenants } from "../../api/tenants";
import { getTenantUsageReport } from "../../api/usage";
import { Badge } from "../../components/ui/badge";
import { MetricCard } from "../../components/ui/metric-card";
import { LoadingState } from "../../components/ui/loading-state";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
import { useAuth } from "../../hooks/useAuth";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { useTimeDisplay } from "../../lib/timeDisplay";
import { AdminSurfaceNav } from "./AdminSurfaceNav";
import { countTone, formatInterval, roleLabel, roleTone } from "./admin-utils";

export function TenantConfigurationPage() {
  const { tenantId, tenantName, environment } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();

  const { data: user, isLoading: authLoading } = useAuth();

  const { data: tenantsData } = useQuery({
    queryKey: ["tenants"],
    queryFn: listTenants,
    retry: false,
  });

  const { data: environmentsData } = useQuery({
    queryKey: ["environments", tenantId],
    queryFn: () => listEnvironments(tenantId),
    enabled: !!tenantId,
    retry: false,
  });

  const { data: usageData, isLoading: usageLoading } = useQuery({
    queryKey: ["tenant-usage-report", tenantId, fromMs, toMs],
    queryFn: () => getTenantUsageReport(tenantId, { from: fromMs, to: toMs }),
    enabled: !!user,
    retry: false,
  });

  if (authLoading || usageLoading || !user || !usageData) {
    return <LoadingState>Loading tenant configuration...</LoadingState>;
  }

  const memberships = user.tenants ?? [];
  const currentMembership = memberships.find((membership) => membership.tenant_id === tenantId);
  const tenantNameById = new Map((tenantsData?.tenants ?? []).map((tenant) => [tenant.id, tenant.name]));
  const environments = environmentsData?.environments ?? [];
  const telemetry = usageData.telemetry_summary;
  const control = usageData.control_plane_summary;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Tenant configuration</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Read-only workspace configuration, RBAC scope, and quota posture for the selected tenant.
            Selected environment: {environment ?? "All envs"}. Usage window: {formatInterval(fromMs, toMs, format)}.
          </p>
        </div>
        <Link
          to="/admin/identity"
          className="text-xs font-bold uppercase tracking-wide text-[var(--brand)] transition-colors hover:text-[var(--text)]"
        >
          Identity settings
        </Link>
      </div>

      <AdminSurfaceNav />

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title="Workspace configuration" eyebrow="Tenant">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <MetricCard label="Tenant" value={tenantName} tone="info" />
            <MetricCard label="Environment" value={environment ?? "All envs"} tone="info" />
            <MetricCard label="Accessible tenants" value={memberships.length} tone={countTone(memberships.length)} />
            <MetricCard
              label="Current role"
              value={roleLabel(currentMembership?.role)}
              tone={currentMembership?.role === "tenant_admin" ? "good" : "info"}
            />
          </div>
        </Panel>

        <Panel title="Quota posture" eyebrow="Usage">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <MetricCard label="Cost index" value={usageData.estimated_cost_index} tone={countTone(usageData.estimated_cost_index)} />
            <MetricCard label="Query reads" value={control.query_reads} tone={countTone(control.query_reads)} />
            <MetricCard label="Telemetry spans" value={telemetry.spans} tone={countTone(telemetry.spans)} />
            <MetricCard label="Metric series" value={telemetry.metric_series_created} tone={countTone(telemetry.metric_series_created)} />
            <MetricCard label="Credential denials" value={control.credential_denies} tone={control.credential_denies > 0 ? "warn" : "good"} />
          </div>
        </Panel>
      </div>

      <Panel
        title="Tenant access"
        eyebrow="RBAC"
        actions={
          <Link
            to="/admin/identity"
            className="text-xs font-semibold uppercase tracking-wide text-[var(--brand)] transition-colors hover:text-[var(--text)]"
          >
            Open identity settings
          </Link>
        }
      >
        <TablePanel>
          <table className="min-w-full border-collapse text-left text-sm">
            <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
              <tr>
                <th scope="col" className="px-3 py-2 font-semibold">Tenant</th>
                <th scope="col" className="px-3 py-2 font-semibold">Role</th>
                <th scope="col" className="px-3 py-2 font-semibold">Scope</th>
              </tr>
            </thead>
            <tbody>
              {memberships.map((membership) => {
                const tenantNameForId = tenantNameById.get(membership.tenant_id) ?? membership.tenant_id;
                const isCurrentTenant = membership.tenant_id === tenantId;

                return (
                  <tr
                    key={membership.tenant_id}
                    className={isCurrentTenant ? "bg-[var(--surface-muted)]/60" : undefined}
                  >
                    <td className="px-3 py-2 font-medium text-[var(--text-strong)]">
                      {tenantNameForId}
                      {isCurrentTenant && (
                        <span className="ml-2 text-[11px] uppercase tracking-wide text-[var(--muted)]">
                          current
                        </span>
                      )}
                    </td>
                    <td className="px-3 py-2">
                      <Badge tone={roleTone(membership.role)}>{roleLabel(membership.role)}</Badge>
                    </td>
                    <td className="px-3 py-2 text-[var(--muted)]">
                      {membership.tenant_id}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </TablePanel>
        <div className="mt-3 text-xs text-[var(--muted)]">
          {environments.length} environment{environments.length === 1 ? "" : "s"} available in the selected tenant.
        </div>
      </Panel>

      <Panel title="Environment scope" eyebrow="Discovery">
        <div className="flex flex-wrap gap-2">
          {environments.map((entry) => (
            <Badge key={entry.environment} tone={entry.environment === environment ? "good" : "neutral"}>
              {entry.environment}
            </Badge>
          ))}
          {environments.length === 0 && (
            <span className="text-sm text-[var(--muted)]">No environments returned for the selected tenant.</span>
          )}
        </div>
      </Panel>
    </section>
  );
}
