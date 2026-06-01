import { Button } from "../../components/ui/button";
import { Panel } from "../../components/ui/panel";
import type { WorkbenchMode, WorkbenchSignal } from "./workbenchState";
import type { WorkbenchQueryState } from "./workbenchRuntime";
import { NotebookEditor } from "./NotebookEditor";
import { NotebookResults } from "./NotebookResults";

interface Props {
  id: string;
  signal: WorkbenchSignal;
  mode: WorkbenchMode;
  draft: string;
  collapsed: boolean;
  runtime: WorkbenchQueryState;
  isActive: boolean;
  onActivate: () => void;
  onModeChange: (mode: WorkbenchMode) => void;
  onDraftChange: (value: string) => void;
  onToggleCollapsed: () => void;
  onRun: () => void;
}

const SIGNAL_LABEL: Record<WorkbenchSignal, string> = {
  metrics: "Metrics",
  logs: "Logs",
  traces: "Traces",
};

export function NotebookBlock({
  id,
  signal,
  mode,
  draft,
  collapsed,
  runtime,
  isActive,
  onActivate,
  onModeChange,
  onDraftChange,
  onToggleCollapsed,
  onRun,
}: Props) {
  return (
    <Panel
      className={[
        "border",
        isActive ? "border-[var(--brand)]" : "border-[var(--border)]",
      ].join(" ")}
      eyebrow={SIGNAL_LABEL[signal]}
      title={`${SIGNAL_LABEL[signal]} block`}
      actions={
        <div className="flex items-center gap-2">
          <div className="flex items-center gap-1 rounded border border-[var(--border)] bg-[var(--surface-subtle)] p-0.5">
            <Button
              variant={mode === "nlq" ? "primary" : "ghost"}
              className="min-h-7 px-2 text-[11px]"
              onClick={() => {
                onActivate();
                onModeChange("nlq");
              }}
            >
              NLQ
            </Button>
            <Button
              variant={mode === "raw" ? "primary" : "ghost"}
              className="min-h-7 px-2 text-[11px]"
              onClick={() => {
                onActivate();
                onModeChange("raw");
              }}
            >
              Raw
            </Button>
          </div>
          <Button variant="secondary" className="min-h-8 px-2 text-xs" onClick={onToggleCollapsed}>
            {collapsed ? "Expand" : "Collapse"}
          </Button>
        </div>
      }
      data-testid={`workbench-block-${id}`}
      onFocusCapture={onActivate}
      onMouseDown={onActivate}
    >
      {collapsed ? (
        <div className="text-sm text-[var(--text-muted)]">
          {SIGNAL_LABEL[signal]} block collapsed.
        </div>
      ) : (
        <div className="space-y-3">
          <NotebookEditor
            value={draft}
            mode={mode}
            onChange={onDraftChange}
            disabled={runtime.status === "loading"}
          />
          <div className="flex flex-wrap items-center gap-2">
            <Button
              onClick={onRun}
              disabled={runtime.status === "loading" || !draft.trim()}
              data-testid={`workbench-run-${id}`}
            >
              {runtime.status === "loading" ? "Running…" : "Run"}
            </Button>
            <span className="text-xs text-[var(--text-muted)]">
              {placeholderForSignal(signal)}
            </span>
          </div>
          <NotebookResults runtime={runtime} />
        </div>
      )}
    </Panel>
  );
}

function placeholderForSignal(signal: WorkbenchSignal): string {
  switch (signal) {
    case "metrics":
      return "Ask about metrics, e.g. p95 latency by service";
    case "logs":
      return "Ask about logs, e.g. error logs for checkout";
    case "traces":
      return "Ask about traces, e.g. slow traces for checkout";
  }
}
