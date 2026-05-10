import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useParams } from "@tanstack/react-router";
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
import { Button } from "../components/ui/button";
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

  function resizePanel(panelId: string, delta: Partial<Pick<DashboardPanelLayout, "w" | "h">>) {
    if (!data) return;
    const panels = data.panels.map((panel) => {
      if (panel.panel_id !== panelId) return panel;
      const nextW = Math.min(12, Math.max(1, panel.layout.w + (delta.w ?? 0)));
      const maxX = Math.max(0, 12 - nextW);
      return {
        ...panel,
        layout: {
          ...panel.layout,
          x: Math.min(panel.layout.x, maxX),
          w: nextW,
          h: Math.max(1, panel.layout.h + (delta.h ?? 0)),
        },
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
            onWiden={() => resizePanel(panel.panel_id, { w: 1 })}
            onNarrow={() => resizePanel(panel.panel_id, { w: -1 })}
            onTaller={() => resizePanel(panel.panel_id, { h: 1 })}
            onShorter={() => resizePanel(panel.panel_id, { h: -1 })}
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
  onWiden,
  onNarrow,
  onTaller,
  onShorter,
}: {
  dashboardId: string;
  panel: DashboardPanel;
  globalRange: { fromMs: number; toMs: number };
  onWiden: () => void;
  onNarrow: () => void;
  onTaller: () => void;
  onShorter: () => void;
}) {
  const minHeight = Math.max(160, panel.layout.h * 80);
  return (
    <div
      className="min-w-0"
      style={{ gridColumn: `span ${Math.min(12, Math.max(1, panel.layout.w))} / span ${Math.min(12, Math.max(1, panel.layout.w))}` }}
    >
      <Panel
        title={panel.title}
        eyebrow={panel.panel_kind === "text" ? "Text" : panel.query_kind ?? "Query"}
        actions={
          <>
            <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onNarrow}>
              Narrow
            </Button>
            <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onWiden}>
              Widen {panel.title}
            </Button>
            <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onShorter}>
              Shorter
            </Button>
            <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onTaller}>
              Taller
            </Button>
          </>
        }
      >
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
