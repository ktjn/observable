import { AdminSurfaceNav } from "./AdminSurfaceNav";
import { Panel } from "../../components/ui/panel";

export function FleetManagementPage() {
  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Administration</div>
          <h1>Fleet management</h1>
          <p className="mt-2 max-w-3xl text-sm text-[var(--muted)]">
            View and manage all Observable agents deployed across your infrastructure.
          </p>
        </div>
      </div>

      <AdminSurfaceNav />

      <Panel title="Fleet management is not yet available" eyebrow="Coming soon">
        <p className="max-w-2xl text-sm text-[var(--muted)]">
          When available, this page will show a live inventory of all agents reporting to your
          tenant — host identity, agent type and version, health status, and applied remote
          configuration version. Agent remote configuration and upgrades will be managed here.
        </p>
      </Panel>
    </section>
  );
}
