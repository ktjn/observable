import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
import type { PointerEvent as ReactPointerEvent } from "react";
import {
  getDashboard,
  updateDashboard,
  type DashboardPanel,
  type DashboardPanelLayout,
  type DashboardPanelTimeRange,
  type UpdateDashboardRequest,
} from "../api/dashboards";
import { submitNlqQuery } from "../api/nlq";
import { VisualizationPanel } from "../features/nlq/VisualizationPanel";
import type { NlqIrLike } from "../features/nlq/queryFilters";
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
    const growBy = Math.min(columns, layout.x, DASHBOARD_GRID_COLUMNS - layout.w);
    return { ...layout, x: layout.x - growBy, w: layout.w + growBy };
  }
  const shrinkBy = Math.min(-columns, layout.w - 1, DASHBOARD_GRID_COLUMNS - layout.x - 1);
  return { ...layout, x: layout.x + shrinkBy, w: layout.w - shrinkBy };
}

function resizeFromBottom(layout: DashboardPanelLayout, rows: number): DashboardPanelLayout {
  if (rows === 0) return layout;
  return { ...layout, h: Math.max(1, layout.h + rows) };
}

export default function DashboardDetailPage() {
  const params = useParams({ strict: false }) as { dashboardId: string };
  const dashboardId = params.dashboardId;
  const { tenantId } = useTenantContext();
  const globalDateRange = useGlobalDateRange();
  const queryClient = useQueryClient();

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
      </div>

      <div className="grid grid-cols-12 gap-3">
        {data.panels.map((panel) => (
          <DashboardPanelView
            key={panel.panel_id}
            dashboardId={data.dashboard_id}
            panel={panel}
            globalRange={{ fromMs: globalDateRange.fromMs, toMs: globalDateRange.toMs }}
            onResizeLeft={(columns) => resizePanel(panel.panel_id, (layout) => resizeFromLeft(layout, columns))}
            onResizeBottom={(rows) => resizePanel(panel.panel_id, (layout) => resizeFromBottom(layout, rows))}
          />
        ))}
      </div>
    </section>
  );
}

function DashboardPanelView({
  dashboardId,
  panel,
  globalRange,
  onResizeLeft,
  onResizeBottom,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
  onResizeLeft: (columns: number) => void;
  onResizeBottom: (rows: number) => void;
}) {
  const minHeight = Math.max(160, panel.layout.h * 80);
  function startLeftResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const startX = event.clientX;
    const onPointerUp = (upEvent: PointerEvent) => {
      const columns = Math.round((startX - upEvent.clientX) / RESIZE_COLUMN_PX);
      onResizeLeft(columns);
      document.removeEventListener("pointerup", onPointerUp);
    };
    document.addEventListener("pointerup", onPointerUp);
  }

  function startBottomResize(event: ReactPointerEvent<HTMLDivElement>) {
    event.preventDefault();
    event.currentTarget.setPointerCapture?.(event.pointerId);
    const startY = event.clientY;
    const onPointerUp = (upEvent: PointerEvent) => {
      const rows = Math.round((upEvent.clientY - startY) / RESIZE_ROW_PX);
      onResizeBottom(rows);
      document.removeEventListener("pointerup", onPointerUp);
    };
    document.addEventListener("pointerup", onPointerUp);
  }

  return (
    <div
      className="min-w-0"
      style={{ gridColumn: `span ${Math.min(DASHBOARD_GRID_COLUMNS, Math.max(1, panel.layout.w))} / span ${Math.min(DASHBOARD_GRID_COLUMNS, Math.max(1, panel.layout.w))}` }}
    >
      <Panel
        title={panel.title}
        eyebrow={panel.panel_kind === "text" ? "Text" : panel.query_kind ?? "Query"}
        className="relative"
      >
        <div
          aria-label={`Resize ${panel.title} from left border`}
          className="absolute top-0 bottom-0 left-0 z-10 w-2 cursor-ew-resize touch-none border-l-2 border-transparent hover:border-[var(--brand)] focus:border-[var(--brand)] focus:outline-none"
          role="separator"
          tabIndex={0}
          onPointerDown={startLeftResize}
        />
        <div
          aria-label={`Resize ${panel.title} from bottom border`}
          className="absolute right-0 bottom-0 left-0 z-10 h-2 cursor-ns-resize touch-none border-b-2 border-transparent hover:border-[var(--brand)] focus:border-[var(--brand)] focus:outline-none"
          role="separator"
          tabIndex={0}
          onPointerDown={startBottomResize}
        />
        <div style={{ minHeight }}>
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
  const baseIr: NlqIrLike = {
    operation: "table",
    signals: [signal],
    filters: [],
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
