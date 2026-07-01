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
import { useTimeDisplay } from "../../lib/timeDisplay";
import { AdminSurfaceNav } from "./AdminSurfaceNav";
import { countTone, formatInterval, roleLabel, roleTone } from "./admin-utils";

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

  const issuer =
    typeof window !== "undefined"
      ? (window as Window & { __OBSERVABLE_ZITADEL_ISSUER__?: string }).__OBSERVABLE_ZITADEL_ISSUER__ ?? "http://localhost:8082"
      : "http://localhost:8082";

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
      </div>

      <AdminSurfaceNav />

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

      <Panel title="Quota posture" eyebrow="Usage">
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <MetricCard label="Cost index" value={data.estimated_cost_index} tone={countTone(data.estimated_cost_index)} />
          <MetricCard label="Query reads" value={control.query_reads} tone={countTone(control.query_reads)} />
          <MetricCard label="Telemetry spans" value={telemetry.spans} tone={countTone(telemetry.spans)} />
          <MetricCard label="Metric series" value={telemetry.metric_series_created} tone={countTone(telemetry.metric_series_created)} />
          <MetricCard label="Credential denials" value={control.credential_denies} tone={control.credential_denies > 0 ? "warn" : "good"} />
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

      <Panel title="Identity provider" eyebrow="Auth">
        <div className="overflow-x-auto">
          <table className="min-w-full border-collapse text-left text-sm">
            <tbody>
              <tr>
                <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Provider</td>
                <td className="py-1.5 text-[var(--text)]">Zitadel 2.71.x</td>
              </tr>
              <tr>
                <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Issuer URL</td>
                <td className="py-1.5">
                  <code className="font-mono text-xs text-[var(--text)]">{issuer}</code>
                </td>
              </tr>
              <tr>
                <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">OIDC Discovery</td>
                <td className="py-1.5">
                  <a
                    href={`${issuer}/.well-known/openid-configuration`}
                    target="_blank"
                    rel="noreferrer"
                    className="font-mono text-xs text-[var(--brand)] hover:text-[var(--text)]"
                  >
                    {issuer}/.well-known/openid-configuration
                  </a>
                </td>
              </tr>
              <tr>
                <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">Redirect URI</td>
                <td className="py-1.5">
                  <code className="font-mono text-xs text-[var(--text)]">
                    {typeof window !== "undefined" ? window.location.origin : ""}/auth/callback
                  </code>
                </td>
              </tr>
              <tr>
                <td className="py-1.5 pr-6 font-semibold text-[var(--text-strong)] whitespace-nowrap">SCIM 2.0 (planned)</td>
                <td className="py-1.5">
                  <code className="font-mono text-xs text-[var(--text)]">{issuer}/scim/v2/&lt;org-id&gt;/</code>
                  <span className="ml-2 text-xs text-[var(--muted)]">— enable per-org in Zitadel Admin Console</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </Panel>
    </section>
  );
}
