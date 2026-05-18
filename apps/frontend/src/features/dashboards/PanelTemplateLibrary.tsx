import { Panel } from "../../components/ui/panel";
import type { DashboardPanelKind, DashboardQueryKind } from "../../api/dashboards";

export interface PanelTemplate {
  id: string;
  title: string;
  description: string;
  panel_kind: DashboardPanelKind;
  query_kind: DashboardQueryKind | null;
  query_text: string | null;
  content: string | null;
  service: string | null;
  icon: string;
}

export const PANEL_TEMPLATES: PanelTemplate[] = [
  {
    id: "error-rate",
    title: "Error rate",
    description: "Timeseries of error rate over time",
    panel_kind: "query",
    query_kind: "metrics",
    query_text: "error rate over time",
    content: null,
    service: null,
    icon: "📉",
  },
  {
    id: "request-rate",
    title: "Request rate",
    description: "Timeseries of request throughput",
    panel_kind: "query",
    query_kind: "metrics",
    query_text: "request rate over time",
    content: null,
    service: null,
    icon: "📈",
  },
  {
    id: "p99-latency",
    title: "P99 latency",
    description: "Latency percentile trend",
    panel_kind: "query",
    query_kind: "metrics",
    query_text: "p99 latency over time",
    content: null,
    service: null,
    icon: "⏱️",
  },
  {
    id: "slow-traces",
    title: "Slow traces",
    description: "Top 10 slowest traces",
    panel_kind: "query",
    query_kind: "traces",
    query_text: "top 10 slowest traces",
    content: null,
    service: null,
    icon: "🔍",
  },
  {
    id: "recent-errors",
    title: "Recent errors",
    description: "Latest error log entries",
    panel_kind: "query",
    query_kind: "logs",
    query_text: "recent errors",
    content: null,
    service: null,
    icon: "🐛",
  },
  {
    id: "cpu-usage",
    title: "CPU usage",
    description: "CPU utilization trend",
    panel_kind: "query",
    query_kind: "metrics",
    query_text: "cpu usage over time",
    content: null,
    service: null,
    icon: "🖥️",
  },
];

export function PanelTemplateLibrary({
  onSelectTemplate,
  onCustomPanel,
}: {
  onSelectTemplate: (template: PanelTemplate) => void;
  onCustomPanel: () => void;
}) {
  return (
    <Panel title="Add panel" eyebrow="Templates">
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {PANEL_TEMPLATES.map((template) => (
          <button
            key={template.id}
            type="button"
            data-testid={`template-${template.id}`}
            className="flex flex-col items-start gap-1 rounded border border-[var(--border)] bg-[var(--surface)] p-3 text-left transition hover:border-[var(--brand)] hover:bg-[var(--surface-raised)] focus:border-[var(--brand)] focus:outline-none"
            onClick={() => onSelectTemplate(template)}
          >
            <span className="text-lg" aria-hidden="true">
              {template.icon}
            </span>
            <span className="text-sm font-semibold text-[var(--text)]">{template.title}</span>
            <span className="text-xs text-[var(--muted)]">{template.description}</span>
          </button>
        ))}
        <button
          type="button"
          data-testid="template-custom"
          className="flex flex-col items-start gap-1 rounded border border-dashed border-[var(--border)] bg-[var(--surface)] p-3 text-left transition hover:border-[var(--brand)] hover:bg-[var(--surface-raised)] focus:border-[var(--brand)] focus:outline-none"
          onClick={onCustomPanel}
        >
          <span className="text-lg" aria-hidden="true">
            ➕
          </span>
          <span className="text-sm font-semibold text-[var(--text)]">Custom panel</span>
          <span className="text-xs text-[var(--muted)]">Build your own query or text panel</span>
        </button>
      </div>
    </Panel>
  );
}
