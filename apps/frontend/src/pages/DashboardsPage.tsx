import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  exportDashboard,
  importDashboard,
  listDashboards,
  type Dashboard,
  type DashboardExport,
  type DashboardPanel,
} from "../api/dashboards";
import { PRESET_OPTIONS } from "../hooks/useGlobalDateRange";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { Panel } from "../components/ui/panel";
import { useTenantContext } from "../hooks/useTenantContext";

export default function DashboardsPage() {
  const { tenantId } = useTenantContext();
  const queryClient = useQueryClient();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importError, setImportError] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["dashboards", tenantId],
    queryFn: () => listDashboards(tenantId),
  });

  async function handleExport(dashboard: Dashboard) {
    try {
      const exported = await exportDashboard(tenantId, dashboard.dashboard_id);
      const blob = new Blob([JSON.stringify(exported, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `${dashboard.name.replace(/\s+/g, "-").toLowerCase()}.dashboard.json`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {
      // export errors are non-fatal; user can retry
    }
  }

  async function handleImportFile(e: React.ChangeEvent<HTMLInputElement>) {
    setImportError(null);
    const file = e.target.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const payload: DashboardExport = JSON.parse(text);
      await importDashboard(tenantId, payload);
      await queryClient.invalidateQueries({ queryKey: ["dashboards", tenantId] });
    } catch (err) {
      setImportError(err instanceof Error ? err.message : "Import failed");
    } finally {
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Saved Views</div>
          <h1>Dashboards</h1>
        </div>
        <div className="flex items-center gap-2">
          <input
            ref={fileInputRef}
            type="file"
            accept=".json"
            className="hidden"
            aria-label="Import dashboard JSON file"
            onChange={handleImportFile}
          />
          <Button variant="secondary" onClick={() => fileInputRef.current?.click()}>
            Import
          </Button>
        </div>
      </div>

      {importError && (
        <p role="alert" className="text-sm text-[var(--bad)]">
          Import failed: {importError}
        </p>
      )}

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
          <Panel
            key={dashboard.dashboard_id}
            title={dashboard.name}
            eyebrow="Dashboard"
            actions={
              <>
                <a
                  href={`/dashboards/${dashboard.dashboard_id}`}
                  className="inline-flex min-h-9 items-center border border-[var(--border)] bg-[var(--surface)] px-3 text-sm font-semibold text-[var(--text)] no-underline hover:border-[var(--brand)]"
                >
                  Open
                </a>
                <Button variant="secondary" onClick={() => handleExport(dashboard)}>
                  Export
                </Button>
              </>
            }
          >
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
  const timeLabel = panel.preset
    ? (PRESET_OPTIONS.find((o) => o.value === panel.preset)?.label ?? panel.preset)
    : "Global date range";
  const kind = panel.panel_kind === "text" ? "text" : (panel.query_kind ?? "query");
  return (
    <div className="border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="text-xs font-bold uppercase text-[var(--muted)]">
        {kind} · {service} · {timeLabel}
      </div>
      <h2 className="mt-2 mb-0 text-base font-bold text-[var(--text-strong)]">{panel.title}</h2>
    </div>
  );
}
