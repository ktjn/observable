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
import { useTenantContext } from "../../hooks/useTenantContext";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTimeDisplay, type TimeFormat } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

type BadgeTone = "good" | "warn" | "bad" | "info" | "neutral";

function formatInterval(fromMs: number, toMs: number, format: TimeFormat): string {
  return `${formatTimestamp(fromMs * 1_000_000, format)} to ${formatTimestamp(toMs * 1_000_000, format)}`;
}

function countTone(value: number): "good" | "warn" | "bad" | "info" {
  if (value === 0) return "good";
  if (value > 1000) return "bad";
  if (value > 100) return "warn";
  return "info";
}

function roleLabel(role?: string): string {
  switch (role) {
    case "tenant_admin":
      return "Tenant admin";
    case "project_admin":
      return "Project admin";
    case "member":
      return "Member";
    case "viewer":
      return "Viewer";
    default:
      return "Unassigned";
  }
}

function roleTone(role?: string): BadgeTone {
  switch (role) {
    case "tenant_admin":
      return "good";
    case "project_admin":
      return "info";
    case "member":
      return "neutral";
    case "viewer":
      return "warn";
    default:
      return "neutral";
  }
}

export function BillingReportPage() {
  const { tenantId, tenantName, environment } = useTenantContext();
  const { fromMs, toMs } = useGlobalDateRange();
  const { format } = useTimeDisplay();

  const { data: meData } = useAuth();

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

  const memberships = meData?.tenants ?? [];
  const currentMembership = memberships.find((membership) => membership.tenant_id === tenantId);
  const tenantNameById = new Map((tenantsData?.tenants ?? []).map((tenant) => [tenant.id, tenant.name]));
  const environments = environmentsData?.environments ?? [];
  const usageTenantId = currentMembership?.tenant_id ?? tenantId;

  const { data, isLoading } = useQuery({
    queryKey: ["tenant-usage-report", usageTenantId, fromMs, toMs],
    queryFn: () => getTenantUsageReport(usageTenantId, { from: fromMs, to: toMs }),
    enabled: !!meData,
  });

  if (isLoading || !data) {
    return <LoadingState>Loading admin console...</LoadingState>;
  }
  const telemetry = data.telemetry_summary;
  const control = data.control_plane_summary;

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Admin Console</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Read-only tenant context, RBAC scope, and usage summary for the selected workspace.
            Selected environment: {environment ?? "All envs"}. Billing interval: {formatInterval(fromMs, toMs, format)}.
          </p>
        </div>
        <Link
          to="/admin/identity"
          className="text-xs font-bold uppercase tracking-wide text-[var(--brand)] transition-colors hover:text-[var(--text)]"
        >
          Identity settings
        </Link>
      </div>

      <Panel title="Tenant context" eyebrow="Workspace">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4">
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

      <Panel title="Identity and access" eyebrow="Settings">
        <p className="max-w-3xl text-sm text-[var(--muted)]">
          Identity provider settings live in the dedicated identity page. This console only
          summarizes the current tenant and role context.
        </p>
      </Panel>

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
