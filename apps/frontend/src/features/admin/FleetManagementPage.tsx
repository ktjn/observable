import { Link } from "@tanstack/react-router";
import { Badge } from "../../components/ui/badge";
import { EmptyState } from "../../components/ui/empty-state";
import { MetricCard } from "../../components/ui/metric-card";
import { Panel } from "../../components/ui/panel";
import { TablePanel } from "../../components/ui/table-panel";
import { useTenantContext } from "../../hooks/useTenantContext";
import { AdminSurfaceNav } from "./AdminSurfaceNav";

type ContractRow = {
  field: string;
  meaning: string;
};

const inventoryFields: ContractRow[] = [
  { field: "agent_id", meaning: "Stable agent identifier derived from host or workload identity." },
  { field: "agent_type", meaning: "infra, language, k8s-operator, browser, mobile, or ebpf." },
  { field: "agent_version", meaning: "Reported runtime version used for compatibility checks." },
  { field: "host_id / cluster / namespace", meaning: "Placement and ownership metadata for the agent." },
  { field: "workload / service_name", meaning: "The workload or service the agent is attached to." },
  { field: "tenant_id / environment", meaning: "Tenant and environment binding used for fleet scoping." },
  { field: "install_time", meaning: "Timestamp of the first registration event." },
];

const heartbeatFields: ContractRow[] = [
  { field: "agent.up", meaning: "Health gauge, 1 or 0." },
  { field: "agent.config_version", meaning: "Current applied remote configuration version." },
  { field: "agent.queue_depth_bytes", meaning: "In-memory buffer usage." },
  { field: "agent.disk_buffer_bytes", meaning: "Disk spill usage when the circuit breaker is open." },
  { field: "agent.dropped_bytes_total", meaning: "Counter by drop reason." },
  { field: "agent.export_errors_total", meaning: "Counter by export failure class." },
  { field: "agent.last_successful_export_timestamp", meaning: "Last successful export instant." },
];

const statusDefinitions = [
  { status: "healthy", condition: "heartbeat within 2x interval and no export errors", tone: "good" as const },
  { status: "degraded", condition: "export errors > 0 or buffer > 50% full", tone: "warn" as const },
  { status: "buffering", condition: "circuit breaker open and writing to disk", tone: "warn" as const },
  { status: "stale", condition: "no heartbeat for 3x interval", tone: "warn" as const },
  { status: "missing", condition: "was healthy, then no heartbeat for 10x interval", tone: "bad" as const },
  { status: "decommissioned", condition: "explicit deregister call received", tone: "neutral" as const },
];

const remoteConfigRows = [
  "Signed, versioned payload delivery over an OpAMP-compatible channel.",
  "Fallback polling every 30 seconds when push is unavailable.",
  "Versioned sampling policies, batch size, flush interval, export endpoints, and log floors.",
  "Automatic rollback to the previous version on apply failure.",
];

const upgradeRows = [
  "stable: tested release for production.",
  "preview: next release candidate for opt-in fleets.",
  "lts: long-term support track for regulated environments.",
];

export function FleetManagementPage() {
  const { tenantName, environment } = useTenantContext();

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Fleet management</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            Read-only agent health and remote configuration contract for the selected workspace.
            Live agent inventory is not wired yet, so this page documents the operational model
            that the backend will populate.
          </p>
        </div>
        <div className="modern-toolbar">
          <Badge tone="warn">Contract view</Badge>
          <Link
            to="/admin/config"
            className="text-xs font-bold uppercase tracking-wide text-[var(--brand)] transition-colors hover:text-[var(--text)]"
          >
            Tenant config
          </Link>
        </div>
      </div>

      <AdminSurfaceNav />

      <div className="grid gap-4 xl:grid-cols-2">
        <MetricCard label="Selected tenant" value={tenantName} tone="info" />
        <MetricCard label="Selected environment" value={environment ?? "All envs"} tone="info" />
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title="Agent inventory fields" eyebrow="Registration">
          <TablePanel>
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
                <tr>
                  <th scope="col" className="px-3 py-2 font-semibold">Field</th>
                  <th scope="col" className="px-3 py-2 font-semibold">Meaning</th>
                </tr>
              </thead>
              <tbody>
                {inventoryFields.map((row) => (
                  <tr key={row.field}>
                    <td className="px-3 py-2 font-medium text-[var(--text-strong)]">
                      {row.field}
                    </td>
                    <td className="px-3 py-2 text-[var(--muted)]">{row.meaning}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TablePanel>
        </Panel>

        <Panel title="Health heartbeat" eyebrow="OTLP metrics">
          <TablePanel>
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
                <tr>
                  <th scope="col" className="px-3 py-2 font-semibold">Metric</th>
                  <th scope="col" className="px-3 py-2 font-semibold">Meaning</th>
                </tr>
              </thead>
              <tbody>
                {heartbeatFields.map((row) => (
                  <tr key={row.field}>
                    <td className="px-3 py-2 font-medium text-[var(--text-strong)]">
                      {row.field}
                    </td>
                    <td className="px-3 py-2 text-[var(--muted)]">{row.meaning}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TablePanel>
        </Panel>
      </div>

      <div className="grid gap-4 xl:grid-cols-2">
        <Panel title="Fleet status definitions" eyebrow="Operations">
          <TablePanel>
            <table className="min-w-full border-collapse text-left text-sm">
              <thead className="bg-[var(--surface-muted)] text-[var(--muted)]">
                <tr>
                  <th scope="col" className="px-3 py-2 font-semibold">Status</th>
                  <th scope="col" className="px-3 py-2 font-semibold">Condition</th>
                </tr>
              </thead>
              <tbody>
                {statusDefinitions.map((row) => (
                  <tr key={row.status}>
                    <td className="px-3 py-2 font-medium text-[var(--text-strong)]">
                      <Badge tone={row.tone}>{row.status}</Badge>
                    </td>
                    <td className="px-3 py-2 text-[var(--muted)]">{row.condition}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </TablePanel>
        </Panel>

        <Panel title="Remote configuration and upgrades" eyebrow="Control plane">
          <div className="grid gap-3">
            <div>
              <div className="field-label">Remote config</div>
              <ul className="mt-2 space-y-2 text-sm text-[var(--muted)]">
                {remoteConfigRows.map((item) => (
                  <li key={item} className="flex gap-2">
                    <span className="mt-1 inline-block h-2 w-2 flex-shrink-0 rounded-full bg-[var(--brand)]" />
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
            </div>

            <div>
              <div className="field-label">Upgrade channels</div>
              <ul className="mt-2 space-y-2 text-sm text-[var(--muted)]">
                {upgradeRows.map((item) => (
                  <li key={item} className="flex gap-2">
                    <span className="mt-1 inline-block h-2 w-2 flex-shrink-0 rounded-full bg-[var(--good)]" />
                    <span>{item}</span>
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </Panel>
      </div>

      <EmptyState
        title="Live agent inventory is not wired yet"
        description="The UI now exposes the fleet contract, but a live agent-status endpoint and remote-config editor are still backend work."
        metadata={[
          "agent.up",
          "agent.config_version",
          "queue depth",
          "disk buffer",
          "last export time",
        ]}
      />
    </section>
  );
}
