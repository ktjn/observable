import { useQuery } from "@tanstack/react-query";
import { listDashboards, type DashboardPanel } from "../api/dashboards";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { Panel } from "../components/ui/panel";

export default function DashboardsPage() {
  const { data, isLoading, isError } = useQuery({
    queryKey: ["dashboards"],
    queryFn: () => listDashboards(),
  });

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Saved Views</div>
          <h1>Dashboards</h1>
        </div>
      </div>

      {isLoading ? (
        <LoadingState>Loading dashboards…</LoadingState>
      ) : isError ? (
        <LoadingState className="text-[var(--bad)]">Dashboards could not be loaded.</LoadingState>
      ) : data?.items.length === 0 ? (
        <EmptyState
          title="No dashboards yet"
          description="Promote a query from Logs or Traces to create the first fixed-layout dashboard."
        />
      ) : (
        data?.items.map((dashboard) => (
          <Panel key={dashboard.dashboard_id} title={dashboard.name} eyebrow="Dashboard">
            <div className="grid gap-3 md:grid-cols-2">
              {dashboard.panels.map((panel) => (
                <DashboardPanelCard key={panel.panel_id} panel={panel} />
              ))}
            </div>
          </Panel>
        ))
      )}
    </section>
  );
}

function DashboardPanelCard({ panel }: { panel: DashboardPanel }) {
  const service = panel.service || "all services";
  return (
    <div className="border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="text-xs font-bold uppercase text-[var(--muted)]">
        {panel.query_kind} · {service} · Last {panel.lookback_minutes}m
      </div>
      <h2 className="mt-2 mb-0 text-base font-bold text-[var(--text-strong)]">{panel.title}</h2>
    </div>
  );
}
