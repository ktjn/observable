import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import {
  getDashboard,
  updateDashboard,
  type DashboardPanel,
  type DashboardPanelKind,
  type DashboardPanelLayout,
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

const DASHBOARD_GRID_COLUMNS = 12;
const RESIZE_COLUMN_PX = 80;
const RESIZE_ROW_PX = 80;

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

function resizeFromLeft(layout: DashboardPanelLayout, columns: number): DashboardPanelLayout {
  if (columns === 0) return layout;
  if (columns > 0) {
    const growBy = Math.min(columns, layout.x);
    return { ...layout, x: layout.x - growBy, w: layout.w + growBy };
  }
  const shrinkBy = Math.min(-columns, layout.w - 1);
  return { ...layout, x: layout.x + shrinkBy, w: layout.w - shrinkBy };
}

function resizeFromBottom(layout: DashboardPanelLayout, rows: number): DashboardPanelLayout {
  if (rows === 0) return layout;
  return { ...layout, h: Math.max(1, layout.h + rows) };
}

function resizeFromRight(layout: DashboardPanelLayout, columns: number): DashboardPanelLayout {
  if (columns === 0) return layout;
  if (columns > 0) {
    const growBy = Math.min(columns, DASHBOARD_GRID_COLUMNS - layout.x - layout.w);
    return { ...layout, w: layout.w + growBy };
  }
  const shrinkBy = Math.min(-columns, layout.w - 1);
  return { ...layout, w: layout.w - shrinkBy };
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

  function resizePanel(panelId: string, updateLayout: (layout: DashboardPanelLayout) => DashboardPanelLayout) {
    if (!data) return;
    const panels = data.panels.map((panel) => {
      if (panel.panel_id !== panelId) return panel;
      return {
        ...panel,
        layout: updateLayout(panel.layout),
      };
    });

    updateMutation.mutate({
      name: data.name,
      panels: panels.map(panelToUpdate),
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

  if (isLoading) return <LoadingState>Loading dashboard...</LoadingState>;
  if (error || !data) {
    return <LoadingState className="text-[var(--bad)]">Dashboard could not be loaded.</LoadingState>;
  }

  return (
    <section className="page-stack">
      <div className="page-header">
        <div>
          <div className="text-xs font-bold uppercase text-[var(--muted)]">Dashboard</div>
          <h1>{data.name}</h1>
        </div>
        <Button variant="primary" onClick={() => setAddPanelOpen((o) => !o)}>
          Add panel
        </Button>
      </div>

      {addPanelOpen && (
        <AddPanelForm
          onAdd={addPanel}
          onCancel={() => setAddPanelOpen(false)}
          isPending={updateMutation.isPending}
        />
      )}

      <div className="grid grid-cols-12 gap-3" style={{ gridAutoRows: `${RESIZE_ROW_PX}px` }}>
        {data.panels.map((panel) => (
          <DashboardPanelView
            key={panel.panel_id}
            dashboardId={data.dashboard_id}
            panel={panel}
            globalRange={{ fromMs: globalDateRange.fromMs, toMs: globalDateRange.toMs }}
            onResizeLeft={(columns) => resizePanel(panel.panel_id, (layout) => resizeFromLeft(layout, columns))}
            onResizeRight={(columns) => resizePanel(panel.panel_id, (layout) => resizeFromRight(layout, columns))}
            onResizeBottom={(rows) => resizePanel(panel.panel_id, (layout) => resizeFromBottom(layout, rows))}
          />
        ))}
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

function DashboardPanelView({
  dashboardId,
  panel,
  globalRange,
  onResizeLeft,
  onResizeRight,
  onResizeBottom,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
  onResizeLeft: (columns: number) => void;
  onResizeRight: (columns: number) => void;
  onResizeBottom: (rows: number) => void;
}) {
  const [previewLayout, setPreviewLayout] = useState<DashboardPanelLayout | null>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  useEffect(() => {
    return () => {
      cleanupRef.current?.();
    };
  }, []);

  const layout = previewLayout ?? panel.layout;
  const x = Math.max(0, Math.min(DASHBOARD_GRID_COLUMNS - 1, layout.x));
  const w = Math.max(1, Math.min(DASHBOARD_GRID_COLUMNS - x, layout.w));
  const h = Math.max(1, layout.h);

  function startLeftResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    cleanupRef.current?.();
    const rect = event.currentTarget.getBoundingClientRect();
    const startEdgeX = rect.width > 0 ? rect.left : event.clientX;
    const startLayout = panel.layout;

    const onPointerMove = (moveEvent: PointerEvent) => {
      const columns = (startEdgeX - moveEvent.clientX) / RESIZE_COLUMN_PX;
      setPreviewLayout(resizeFromLeft(startLayout, columns));
    };

    const onPointerUp = (upEvent: PointerEvent) => {
      const columns = Math.round((startEdgeX - upEvent.clientX) / RESIZE_COLUMN_PX);
      onResizeLeft(columns);
      setPreviewLayout(null);
      cleanup();
    };

    const cleanup = () => {
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerUp);
      cleanupRef.current = null;
    };

    cleanupRef.current = cleanup;
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerUp);
  }

  function startRightResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    cleanupRef.current?.();
    const rect = event.currentTarget.getBoundingClientRect();
    const startEdgeX = rect.width > 0 ? rect.right : event.clientX;
    const startLayout = panel.layout;

    const onPointerMove = (moveEvent: PointerEvent) => {
      const columns = (moveEvent.clientX - startEdgeX) / RESIZE_COLUMN_PX;
      setPreviewLayout(resizeFromRight(startLayout, columns));
    };

    const onPointerUp = (upEvent: PointerEvent) => {
      const columns = Math.round((upEvent.clientX - startEdgeX) / RESIZE_COLUMN_PX);
      onResizeRight(columns);
      setPreviewLayout(null);
      cleanup();
    };

    const cleanup = () => {
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerUp);
      cleanupRef.current = null;
    };

    cleanupRef.current = cleanup;
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerUp);
  }

  function startBottomResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    cleanupRef.current?.();
    const rect = event.currentTarget.getBoundingClientRect();
    const startEdgeY = rect.height > 0 ? rect.bottom : event.clientY;
    const startLayout = panel.layout;

    const onPointerMove = (moveEvent: PointerEvent) => {
      const rows = (moveEvent.clientY - startEdgeY) / RESIZE_ROW_PX;
      setPreviewLayout(resizeFromBottom(startLayout, rows));
    };

    const onPointerUp = (upEvent: PointerEvent) => {
      const rows = Math.round((upEvent.clientY - startEdgeY) / RESIZE_ROW_PX);
      onResizeBottom(rows);
      setPreviewLayout(null);
      cleanup();
    };

    const cleanup = () => {
      document.removeEventListener("pointermove", onPointerMove);
      document.removeEventListener("pointerup", onPointerUp);
      document.removeEventListener("pointercancel", onPointerUp);
      cleanupRef.current = null;
    };

    cleanupRef.current = cleanup;
    document.addEventListener("pointermove", onPointerMove);
    document.addEventListener("pointerup", onPointerUp);
    document.addEventListener("pointercancel", onPointerUp);
  }

  return (
    <div
      className="min-w-0"
      style={{
        gridColumnStart: x + 1,
        gridColumnEnd: `span ${w}`,
        gridRowStart: layout.y + 1,
        gridRowEnd: `span ${h}`,
      }}
    >
      <Panel
        title={panel.title}
        eyebrow={panel.panel_kind === "text" ? "Text" : panel.query_kind ?? "Query"}
        className="relative h-full"
      >
        <div
          aria-label={`Resize ${panel.title} from left border`}
          className="group absolute top-0 bottom-0 left-0 z-10 w-3 cursor-ew-resize touch-none border-l-2 border-transparent hover:border-[var(--brand)] focus:border-[var(--brand)] focus:outline-none"
          role="separator"
          tabIndex={0}
          onPointerDown={startLeftResize}
        >
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div className="h-5 w-0.5 rounded-full bg-[var(--brand)] opacity-0 transition-opacity group-hover:opacity-100" />
          </div>
        </div>
        <div
          aria-label={`Resize ${panel.title} from right border`}
          className="group absolute top-0 right-0 bottom-0 z-10 w-3 cursor-ew-resize touch-none border-r-2 border-transparent hover:border-[var(--brand)] focus:border-[var(--brand)] focus:outline-none"
          role="separator"
          tabIndex={0}
          onPointerDown={startRightResize}
        >
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div className="h-5 w-0.5 rounded-full bg-[var(--brand)] opacity-0 transition-opacity group-hover:opacity-100" />
          </div>
        </div>
        <div
          aria-label={`Resize ${panel.title} from bottom border`}
          className="group absolute right-0 bottom-0 left-0 z-10 h-3 cursor-ns-resize touch-none border-b-2 border-transparent hover:border-[var(--brand)] focus:border-[var(--brand)] focus:outline-none"
          role="separator"
          tabIndex={0}
          onPointerDown={startBottomResize}
        >
          <div className="pointer-events-none absolute inset-0 flex items-center justify-center">
            <div className="h-0.5 w-5 rounded-full bg-[var(--brand)] opacity-0 transition-opacity group-hover:opacity-100" />
          </div>
        </div>
        <div className="h-full min-h-[80px]">
          {panel.panel_kind === "text" ? (
            <TextPanel panel={panel} />
          ) : (
            <QueryPanel dashboardId={dashboardId} panel={panel} globalRange={globalRange} />
          )}
        </div>
      </Panel>
    </div>
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
