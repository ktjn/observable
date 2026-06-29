import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import {
  createDashboard,
  deleteDashboard,
  exportDashboard,
  importDashboard,
  listDashboards,
  type Dashboard,
  type DashboardExport,
} from "../api/dashboards";
import { Badge } from "../components/ui/badge";
import { Button } from "../components/ui/button";
import { EmptyState } from "../components/ui/empty-state";
import { LoadingState } from "../components/ui/loading-state";
import { useTenantContext } from "../hooks/useTenantContext";

export default function DashboardsPage() {
  const { tenantId } = useTenantContext();
  const queryClient = useQueryClient();
  const router = useRouter();
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [importError, setImportError] = useState<string | null>(null);

  // Create affordance state
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [createSubmitting, setCreateSubmitting] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);

  const { data, isLoading, isError } = useQuery({
    queryKey: ["dashboards", tenantId],
    queryFn: () => listDashboards(tenantId),
  });

  const deleteMutation = useMutation({
    mutationFn: (dashboardId: string) => deleteDashboard(tenantId, dashboardId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["dashboards", tenantId] }),
  });

  async function handleCreate() {
    if (!newName.trim()) return;
    setCreateSubmitting(true);
    setCreateError(null);
    try {
      const created = await createDashboard(tenantId, { name: newName.trim(), panels: [] });
      await queryClient.invalidateQueries({ queryKey: ["dashboards", tenantId] });
      router.navigate({ to: `/dashboards/${created.dashboard_id}` });
      setCreating(false);
      setNewName("");
    } catch (err) {
      setCreateError(err instanceof Error ? err.message : "Create failed");
    } finally {
      setCreateSubmitting(false);
    }
  }

  function handleCancelCreate() {
    setCreating(false);
    setNewName("");
    setCreateError(null);
  }

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
          {creating ? (
            <>
              <input
                type="text"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                placeholder="Dashboard name"
                aria-label="New dashboard name"
                className="px-2.5 py-1 text-sm border border-[var(--border)] bg-transparent text-[var(--text)] placeholder:text-[var(--muted)] focus:outline-none focus:border-[var(--brand)]"
                onKeyDown={(e) => e.key === "Enter" && handleCreate()}
                autoFocus
              />
              <Button onClick={handleCreate} disabled={createSubmitting}>
                {createSubmitting ? "Creating…" : "Create"}
              </Button>
              <Button variant="secondary" onClick={handleCancelCreate}>
                Cancel
              </Button>
              {createError && (
                <span className="text-sm text-[var(--bad)]">{createError}</span>
              )}
            </>
          ) : (
            <>
              <Button onClick={() => setCreating(true)}>New dashboard</Button>
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
            </>
          )}
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
  const formattedDate = new Date(dashboard.created_at).toLocaleDateString();

  return (
    <div className="flex flex-col border border-[var(--border)] bg-[var(--surface)] p-4">
      <div className="mb-1">
        <div className="flex items-center gap-2 text-xs font-bold uppercase text-[var(--muted)]">
          <span>
            {panelCount} {panelCount === 1 ? "panel" : "panels"}
          </span>
          <Badge tone="info">{dashboard.visibility}</Badge>
        </div>
        <h2 className="m-0 text-base font-bold text-[var(--text-strong)]">{dashboard.name}</h2>
        <div className="mt-0.5 text-xs text-[var(--muted)]">Created {formattedDate}</div>
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
