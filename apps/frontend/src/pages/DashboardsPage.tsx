import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  deleteDashboard,
  exportDashboard,
  importDashboard,
  listDashboards,
  type Dashboard,
  type DashboardExport,
} from "../api/dashboards";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
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

  const deleteMutation = useMutation({
    mutationFn: (dashboardId: string) => deleteDashboard(tenantId, dashboardId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["dashboards", tenantId] }),
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
          description="Create your first dashboard by promoting a query from Logs, Traces, or Metrics."
        />
      ) : (
        <div
          className="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3"
          aria-label="Dashboard cards"
        >
          {data?.items.map((dashboard) => (
            <DashboardCard
              key={dashboard.dashboard_id}
              dashboard={dashboard}
              onExport={() => handleExport(dashboard)}
              onDelete={() => {
                if (confirm(`Delete dashboard "${dashboard.name}"?`)) {
                  deleteMutation.mutate(dashboard.dashboard_id);
                }
              }}
              deletePending={deleteMutation.isPending}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function DashboardCard({
  dashboard,
  onExport,
  onDelete,
  deletePending,
}: {
  dashboard: Dashboard;
  onExport: () => void;
  onDelete: () => void;
  deletePending: boolean;
}) {
  const panelCount = dashboard.panels.length;
  return (
    <div className="flex flex-col border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="mb-1">
        <div className="text-xs font-bold uppercase text-[var(--muted)]">
          {panelCount} {panelCount === 1 ? "panel" : "panels"}
        </div>
        <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">{dashboard.name}</h2>
      </div>

      <div className="mt-auto flex items-center gap-2 pt-4">
        <a
          href={`/dashboards/${dashboard.dashboard_id}`}
          className="inline-flex min-h-9 items-center border border-[var(--border)] bg-[var(--surface)] px-3 text-sm font-semibold text-[var(--text)] no-underline hover:border-[var(--brand)]"
        >
          Open
        </a>
        <Button variant="secondary" onClick={onExport}>
          Export
        </Button>
        <Button variant="secondary" onClick={onDelete} disabled={deletePending}>
          Delete
        </Button>
      </div>
    </div>
  );
}
