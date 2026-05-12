import { GridLayout, useContainerWidth, type LayoutItem } from 'react-grid-layout';
import 'react-grid-layout/css/styles.css';
import 'react-resizable/css/styles.css';
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { useState } from "react";
import {
  getDashboard,
  updateDashboard,
  type DashboardPanel,
  type DashboardPanelKind,
  type DashboardPanelTimeRange,
  type DashboardQueryKind,
  type UpdateDashboardRequest,
} from "../api/dashboards";
import { submitNlqQuery } from "../api/nlq";
import { VisualizationPanel } from "../features/nlq/VisualizationPanel";
import type { NlqIrLike } from "../features/nlq/queryFilters";
import { Button } from "../components/ui/button";
import { Input } from "../components/ui/input";
import { Select, SelectOption } from "../components/ui/select";
import { LoadingState } from "../components/ui/loading-state";
import { Panel } from "../components/ui/panel";
import { presetToMs, useGlobalDateRange } from "../hooks/useGlobalDateRange";
import { useTenantContext } from "../hooks/useTenantContext";

function msToNsString(ms: number): string {
  return String(BigInt(Math.floor(ms)) * 1_000_000n);
}

function resolvePanelTimeRange(
  timeRange: DashboardPanelTimeRange,
  globalRange: { fromMs: number; toMs: number },
): { fromMs: number; toMs: number } {
  if (timeRange.mode === "absolute") {
    return { fromMs: timeRange.from_ms, toMs: timeRange.to_ms };
  }
  if (timeRange.mode === "preset") {
    const toMs = Date.now();
    return { fromMs: toMs - presetToMs(timeRange.preset), toMs };
  }
  return globalRange;
}

function panelToUpdate(panel: DashboardPanel): UpdateDashboardRequest["panels"][number] {
  return {
    panel_id: panel.panel_id,
    title: panel.title,
    panel_kind: panel.panel_kind,
    query_kind: panel.query_kind,
    service: panel.service,
    preset: panel.preset,
    filters: panel.filters,
    query_text: panel.query_text,
    content: panel.content,
    layout: panel.layout,
    time_range: panel.time_range,
  };
}

function nextRowAfterPanels(panels: DashboardPanel[]): number {
  if (panels.length === 0) return 0;
  return Math.max(...panels.map((p) => p.layout.y + p.layout.h));
}

function dashboardFiltersToNlqFilters(filters: Record<string, unknown>): NonNullable<NlqIrLike["filters"]> {
  const result: NonNullable<NlqIrLike["filters"]> = [];
  const name = stringFilter(filters.name);
  const type = stringFilter(filters.type);
  const environment = stringFilter(filters.environment);

  if (name) result.push({ field: "metric_name", op: "=", value: name });
  if (type && type !== "all") result.push({ field: "metric_type", op: "=", value: type });
  if (environment && environment !== "all") {
    result.push({ field: "environment", op: "=", value: environment });
  }

  return result;
}

function stringFilter(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

export default function DashboardDetailPage() {
  const params = useParams({ strict: false }) as { dashboardId: string };
  const dashboardId = params.dashboardId;
  const { tenantId } = useTenantContext();
  const globalDateRange = useGlobalDateRange();
  const queryClient = useQueryClient();
  const [addPanelOpen, setAddPanelOpen] = useState(false);
  const [editingPanelId, setEditingPanelId] = useState<string | null>(null);
  const [editingName, setEditingName] = useState(false);
  const [editMode, setEditMode] = useState(false);
  const [stagedLayout, setStagedLayout] = useState<LayoutItem[] | null>(null);
  const { width, containerRef, mounted } = useContainerWidth({ initialWidth: 1200 });

  const { data, isLoading, error } = useQuery({
    queryKey: ["dashboard", tenantId, dashboardId],
    queryFn: () => getDashboard(tenantId, dashboardId),
  });

  const updateMutation = useMutation({
    mutationFn: (next: UpdateDashboardRequest) => updateDashboard(tenantId, dashboardId, next),
    onSuccess: (updated) => {
      queryClient.setQueryData(["dashboard", tenantId, dashboardId], updated);
    },
  });

  function enterEditMode() {
    setEditMode(true);
  }

  function saveLayout() {
    if (!data) { setEditMode(false); return; }
    const panels = data.panels.map((panel) => {
      const staged = stagedLayout?.find((l) => l.i === panel.panel_id);
      if (!staged) return panelToUpdate(panel);
      return { ...panelToUpdate(panel), layout: { x: staged.x, y: staged.y, w: staged.w, h: staged.h } };
    });
    updateMutation.mutate(
      { name: data.name, panels },
      {
        onSuccess: () => {
          setStagedLayout(null);
          setEditMode(false);
        },
      },
    );
  }

  function cancelEdit() {
    setStagedLayout(null);
    setEditMode(false);
  }

  function deletePanel(panelId: string) {
    if (!data) return;
    updateMutation.mutate({
      name: data.name,
      panels: data.panels.filter((p) => p.panel_id !== panelId).map(panelToUpdate),
    });
  }

  function addPanel(newPanel: {
    title: string;
    panel_kind: DashboardPanelKind;
    query_kind: DashboardQueryKind | null;
    service: string | null;
    query_text: string | null;
    content: string | null;
  }) {
    if (!data) return;
    const y = nextRowAfterPanels(data.panels);
    const panel: UpdateDashboardRequest["panels"][number] = {
      title: newPanel.title,
      panel_kind: newPanel.panel_kind,
      query_kind: newPanel.query_kind,
      service: newPanel.service,
      preset: null,
      filters: {},
      query_text: newPanel.query_text,
      content: newPanel.content,
      layout: { x: 0, y, w: 12, h: 4 },
      time_range: { mode: "global" },
    };
    updateMutation.mutate({
      name: data.name,
      panels: [...data.panels.map(panelToUpdate), panel],
    });
    setAddPanelOpen(false);
  }

  function editPanel(
    panelId: string,
    changes: {
      title: string;
      panel_kind: DashboardPanelKind;
      query_kind: DashboardQueryKind | null;
      service: string | null;
      query_text: string | null;
      content: string | null;
    },
  ) {
    if (!data) return;
    updateMutation.mutate({
      name: data.name,
      panels: data.panels.map((p) =>
        p.panel_id !== panelId
          ? panelToUpdate(p)
          : {
              ...panelToUpdate(p),
              title: changes.title,
              panel_kind: changes.panel_kind,
              query_kind: changes.query_kind,
              service: changes.service,
              query_text: changes.query_text,
              content: changes.content,
            },
      ),
    });
    setEditingPanelId(null);
  }

  function renameDashboard(name: string) {
    if (!data || !name.trim() || name.trim() === data.name) {
      setEditingName(false);
      return;
    }
    updateMutation.mutate({ name: name.trim(), panels: data.panels.map(panelToUpdate) });
    setEditingName(false);
  }

  if (isLoading) return <LoadingState>Loading dashboard...</LoadingState>;
  if (error || !data) {
    return <LoadingState className="text-[var(--bad)]">Dashboard could not be loaded.</LoadingState>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Dashboard</div>
          {editingName ? (
            <InlineNameEditor
              initialValue={data.name}
              onSave={renameDashboard}
              onCancel={() => setEditingName(false)}
            />
          ) : (
            <h1
              className="cursor-pointer hover:opacity-70"
              title="Click to rename"
              onClick={() => setEditingName(true)}
            >
              {data.name}
            </h1>
          )}
        </div>
        <div className="flex items-center gap-2">
          {editMode ? (
            <>
              <Button variant="primary" onClick={saveLayout} disabled={updateMutation.isPending}>
                {updateMutation.isPending ? "Saving…" : "Done"}
              </Button>
              <Button variant="secondary" onClick={cancelEdit}>
                Cancel
              </Button>
            </>
          ) : (
            <>
              <Button variant="secondary" onClick={enterEditMode}>
                Edit layout
              </Button>
              <Button variant="primary" onClick={() => setAddPanelOpen((o) => !o)}>
                Add panel
              </Button>
            </>
          )}
        </div>
      </div>

      {addPanelOpen && (
        <AddPanelForm
          onAdd={addPanel}
          onCancel={() => setAddPanelOpen(false)}
          isPending={updateMutation.isPending}
        />
      )}

      <div ref={containerRef}>
        {mounted && (
          <GridLayout
            width={width}
            gridConfig={{ cols: 12, rowHeight: 100, margin: [12, 12] as [number, number] }}
            dragConfig={{ enabled: editMode, handle: ".panel-drag-handle" }}
            resizeConfig={{ enabled: editMode }}
            layout={
              stagedLayout ??
              data.panels.map((p) => ({
                i: p.panel_id,
                x: p.layout.x,
                y: p.layout.y,
                w: p.layout.w,
                h: p.layout.h,
                minW: 2,
                minH: 1,
              }))
            }
            onLayoutChange={(layout) => {
              if (editMode) setStagedLayout([...layout]);
            }}
          >
            {data.panels.map((panel) =>
              editingPanelId === panel.panel_id ? (
                <div key={panel.panel_id} className="min-w-0">
                  <EditPanelForm
                    panel={panel}
                    onSave={(changes) => editPanel(panel.panel_id, changes)}
                    onCancel={() => setEditingPanelId(null)}
                    isPending={updateMutation.isPending}
                  />
                </div>
              ) : (
                <div key={panel.panel_id} className="min-w-0">
                  <DashboardPanelView
                    dashboardId={data.dashboard_id}
                    panel={panel}
                    globalRange={{ fromMs: globalDateRange.fromMs, toMs: globalDateRange.toMs }}
                    editMode={editMode}
                    onDelete={() => deletePanel(panel.panel_id)}
                    onEdit={() => setEditingPanelId(panel.panel_id)}
                  />
                </div>
              ),
            )}
          </GridLayout>
        )}
      </div>
    </section>
  );
}

function AddPanelForm({
  onAdd,
  onCancel,
  isPending,
}: {
  onAdd: (panel: {
    title: string;
    panel_kind: DashboardPanelKind;
    query_kind: DashboardQueryKind | null;
    service: string | null;
    query_text: string | null;
    content: string | null;
  }) => void;
  onCancel: () => void;
  isPending: boolean;
}) {
  const [title, setTitle] = useState("");
  const [panelKind, setPanelKind] = useState<DashboardPanelKind>("query");
  const [queryKind, setQueryKind] = useState<DashboardQueryKind>("logs");
  const [service, setService] = useState("");
  const [queryText, setQueryText] = useState("");
  const [content, setContent] = useState("");

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    onAdd({
      title: title.trim(),
      panel_kind: panelKind,
      query_kind: panelKind === "query" ? queryKind : null,
      service: panelKind === "query" && service.trim() ? service.trim() : null,
      query_text: panelKind === "query" && queryText.trim() ? queryText.trim() : null,
      content: panelKind === "text" ? content : null,
    });
  }

  return (
    <Panel title="New panel" eyebrow="Add">
      <form onSubmit={handleSubmit} className="grid gap-3">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Title</span>
            <Input
              required
              placeholder="Panel title"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Kind</span>
            <Select value={panelKind} onChange={(e) => setPanelKind(e.target.value as DashboardPanelKind)}>
              <SelectOption value="query">Query</SelectOption>
              <SelectOption value="text">Text</SelectOption>
            </Select>
          </label>
          {panelKind === "query" && (
            <>
              <label className="flex flex-col gap-1">
                <span className="text-xs font-semibold uppercase text-[var(--muted)]">Signal</span>
                <Select value={queryKind} onChange={(e) => setQueryKind(e.target.value as DashboardQueryKind)}>
                  <SelectOption value="logs">Logs</SelectOption>
                  <SelectOption value="traces">Traces</SelectOption>
                  <SelectOption value="metrics">Metrics</SelectOption>
                </Select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs font-semibold uppercase text-[var(--muted)]">Service</span>
                <Input
                  placeholder="Optional"
                  value={service}
                  onChange={(e) => setService(e.target.value)}
                />
              </label>
            </>
          )}
        </div>
        {panelKind === "query" ? (
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Query</span>
            <Input
              placeholder="Natural language question, e.g. error rate over time"
              value={queryText}
              onChange={(e) => setQueryText(e.target.value)}
            />
          </label>
        ) : (
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Content</span>
            <textarea
              className="min-h-[80px] w-full resize-y border border-[var(--border-strong)] bg-[var(--surface-raised)] px-2 py-1 font-[family-name:'IBM_Plex_Mono',monospace] text-[11px] text-[var(--text)] outline-none focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)]"
              placeholder="Panel text content"
              value={content}
              onChange={(e) => setContent(e.target.value)}
            />
          </label>
        )}
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={!title.trim() || isPending}>
            {isPending ? "Saving…" : "Add panel"}
          </Button>
          <Button type="button" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </form>
    </Panel>
  );
}

function EditPanelForm({
  panel,
  onSave,
  onCancel,
  isPending,
}: {
  panel: DashboardPanel;
  onSave: (changes: {
    title: string;
    panel_kind: DashboardPanelKind;
    query_kind: DashboardQueryKind | null;
    service: string | null;
    query_text: string | null;
    content: string | null;
  }) => void;
  onCancel: () => void;
  isPending: boolean;
}) {
  const [title, setTitle] = useState(panel.title);
  const [panelKind, setPanelKind] = useState<DashboardPanelKind>(panel.panel_kind);
  const [queryKind, setQueryKind] = useState<DashboardQueryKind>((panel.query_kind as DashboardQueryKind) ?? "logs");
  const [service, setService] = useState(panel.service ?? "");
  const [queryText, setQueryText] = useState(panel.query_text ?? "");
  const [content, setContent] = useState(panel.content ?? "");

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!title.trim()) return;
    onSave({
      title: title.trim(),
      panel_kind: panelKind,
      query_kind: panelKind === "query" ? queryKind : null,
      service: panelKind === "query" && service.trim() ? service.trim() : null,
      query_text: panelKind === "query" && queryText.trim() ? queryText.trim() : null,
      content: panelKind === "text" ? content : null,
    });
  }

  return (
    <Panel title={panel.title} eyebrow="Edit">
      <form onSubmit={handleSubmit} className="grid gap-3">
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Title</span>
            <Input required placeholder="Panel title" value={title} onChange={(e) => setTitle(e.target.value)} />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Kind</span>
            <Select value={panelKind} onChange={(e) => setPanelKind(e.target.value as DashboardPanelKind)}>
              <SelectOption value="query">Query</SelectOption>
              <SelectOption value="text">Text</SelectOption>
            </Select>
          </label>
          {panelKind === "query" && (
            <>
              <label className="flex flex-col gap-1">
                <span className="text-xs font-semibold uppercase text-[var(--muted)]">Signal</span>
                <Select value={queryKind} onChange={(e) => setQueryKind(e.target.value as DashboardQueryKind)}>
                  <SelectOption value="logs">Logs</SelectOption>
                  <SelectOption value="traces">Traces</SelectOption>
                  <SelectOption value="metrics">Metrics</SelectOption>
                </Select>
              </label>
              <label className="flex flex-col gap-1">
                <span className="text-xs font-semibold uppercase text-[var(--muted)]">Service</span>
                <Input placeholder="Optional" value={service} onChange={(e) => setService(e.target.value)} />
              </label>
            </>
          )}
        </div>
        {panelKind === "query" ? (
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Query</span>
            <Input
              placeholder="Natural language question, e.g. error rate over time"
              value={queryText}
              onChange={(e) => setQueryText(e.target.value)}
            />
          </label>
        ) : (
          <label className="flex flex-col gap-1">
            <span className="text-xs font-semibold uppercase text-[var(--muted)]">Content</span>
            <textarea
              className="min-h-[80px] w-full resize-y border border-[var(--border-strong)] bg-[var(--surface-raised)] px-2 py-1 font-[family-name:'IBM_Plex_Mono',monospace] text-[11px] text-[var(--text)] outline-none focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)]"
              placeholder="Panel text content"
              value={content}
              onChange={(e) => setContent(e.target.value)}
            />
          </label>
        )}
        <div className="flex gap-2">
          <Button type="submit" variant="primary" disabled={!title.trim() || isPending}>
            {isPending ? "Saving…" : "Save"}
          </Button>
          <Button type="button" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </form>
    </Panel>
  );
}

function InlineNameEditor({
  initialValue,
  onSave,
  onCancel,
}: {
  initialValue: string;
  onSave: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initialValue);

  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") onSave(value);
    if (e.key === "Escape") onCancel();
  }

  return (
    <Input
      autoFocus
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onBlur={() => onSave(value)}
      onKeyDown={handleKeyDown}
      className="text-xl font-bold"
    />
  );
}

function DashboardPanelView({
  dashboardId,
  panel,
  globalRange,
  editMode,
  onDelete,
  onEdit,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
  editMode: boolean;
  onDelete: () => void;
  onEdit: () => void;
}) {
  return (
    <Panel
      title={panel.title}
      eyebrow={panel.panel_kind === "text" ? "Text" : (panel.query_kind ?? "Query")}
      className="h-full"
      actions={
        <>
          {editMode && (
            <div
              className="panel-drag-handle flex h-7 w-7 cursor-grab items-center justify-center touch-none text-[var(--muted)] hover:text-[var(--text)] active:cursor-grabbing select-none"
              title="Drag to move"
            >
              ⠿
            </div>
          )}
          <button
            type="button"
            aria-label={`Edit panel ${panel.title}`}
            className="flex h-7 items-center px-2 text-xs text-[var(--muted)] hover:text-[var(--text)] focus:outline-none"
            onClick={onEdit}
          >
            Edit
          </button>
          <button
            type="button"
            aria-label={`Delete panel ${panel.title}`}
            className="flex h-7 w-7 items-center justify-center text-[var(--muted)] hover:text-[var(--bad)] focus:outline-none"
            onClick={onDelete}
          >
            ×
          </button>
        </>
      }
    >
      <div className="h-full min-h-[100px]">
        {panel.panel_kind === "text" ? (
          <TextPanel panel={panel} />
        ) : (
          <QueryPanel dashboardId={dashboardId} panel={panel} globalRange={globalRange} />
        )}
      </div>
    </Panel>
  );
}

function TextPanel({ panel }: { panel: DashboardPanel }) {
  return (
    <p className="m-0 whitespace-pre-wrap text-sm leading-6 text-[var(--text)]">
      {panel.content ?? ""}
    </p>
  );
}

function QueryPanel({
  dashboardId,
  panel,
  globalRange,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
}) {
  const { tenantId } = useTenantContext();
  const resolved = resolvePanelTimeRange(panel.time_range, globalRange);
  const signal = panel.query_kind ?? "logs";
  const hasQuestion = Boolean(panel.query_text?.trim());
  const baseIr: NlqIrLike = {
    operation: signal === "metrics" && !hasQuestion ? "catalog" : "table",
    signals: [signal],
    catalog_field: signal === "metrics" && !hasQuestion ? "metric_name" : undefined,
    filters: signal === "metrics" ? dashboardFiltersToNlqFilters(panel.filters) : [],
    time_range: {
      from: msToNsString(resolved.fromMs),
      to: msToNsString(resolved.toMs),
    },
  };

  const { data, isLoading, error } = useQuery({
    queryKey: [
      "dashboard-panel",
      tenantId,
      dashboardId,
      panel.panel_id,
      panel.query_text,
      panel.service,
      resolved.fromMs,
      resolved.toMs,
    ],
    queryFn: () =>
      submitNlqQuery(tenantId, {
        question: panel.query_text ?? undefined,
        mode: "execute",
        service_name: panel.service ?? undefined,
        base_ir: baseIr,
      }),
  });

  if (isLoading) return <LoadingState>Loading panel...</LoadingState>;
  if (error) {
    return <LoadingState className="text-[var(--bad)]">Panel query failed: {String(error)}</LoadingState>;
  }
  if (!data || data.type !== "frame") {
    return <LoadingState>No panel data returned.</LoadingState>;
  }
  return (
    <div className="grid gap-2">
      <VisualizationPanel frame={data.frame} />
      <p className="m-0 text-xs italic text-[var(--muted)]">{data.frame.approximation_statement}</p>
    </div>
  );
}
