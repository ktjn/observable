import { useEffect, useMemo, useState } from "react";
import { useNavigate, useSearch } from "@tanstack/react-router";
import type { NlqIrLike } from "../../api/nlq";
import { useQuery } from "@tanstack/react-query";
import { getConfig } from "../../api/setup";
import { submitNlqWithProvider } from "../nlq/submitNlqWithProvider";
import {
  createIdleWorkbenchQueryStateMap,
  type WorkbenchQueryState as WorkbenchQueryStateRuntime,
} from "./workbenchRuntime";
import { createStarterWorkbenchState, decodeWorkbenchState, encodeWorkbenchState, type NotebookStateV1, type WorkbenchSignal } from "./workbenchState";
import { NotebookBlock } from "./NotebookBlock";
import { useGlobalDateRange } from "../../hooks/useGlobalDateRange";
import { useTenantContext } from "../../hooks/useTenantContext";
import { Button } from "../../components/ui/button";

const BASE_IR_BY_SIGNAL: Record<WorkbenchSignal, NlqIrLike> = {
  metrics: {
    operation: "catalog",
    signals: ["metrics"],
    filters: [],
    time_range: { from: "now-1h", to: "now" },
  },
  logs: {
    operation: "table",
    signals: ["logs"],
    filters: [],
    time_range: { from: "now-1h", to: "now" },
  },
  traces: {
    operation: "table",
    signals: ["traces"],
    filters: [],
    time_range: { from: "now-1h", to: "now" },
  },
};

export type WorkbenchQueryState = WorkbenchQueryStateRuntime;

export default function QueryWorkbench() {
  const search = useSearch({ strict: false }) as { state?: string };
  const navigate = useNavigate();
  const { fromMs, toMs } = useGlobalDateRange();
  const { tenantId } = useTenantContext();
  const { data: config } = useQuery({
    queryKey: ["setup", "config", tenantId],
    queryFn: () => getConfig(tenantId),
  });
  const provider = config?.llm_provider ?? "remote";

  const [notebook, setNotebook] = useState<NotebookStateV1>(() =>
    decodeWorkbenchState(search.state),
  );
  const [runStateById, setRunStateById] = useState<Record<string, WorkbenchQueryState>>(() =>
    createIdleWorkbenchQueryStateMap(notebook.blocks.map((block) => block.id)),
  );

  useEffect(() => {
    void navigate({
      search: (prev: Record<string, unknown>) => ({
        ...prev,
        state: encodeWorkbenchState(notebook),
      }),
      replace: true,
    } as unknown as Parameters<typeof navigate>[0]);
  }, [navigate, notebook]);

  const blockById = useMemo(
    () => new Map(notebook.blocks.map((block) => [block.id, block] as const)),
    [notebook.blocks],
  );

  function updateNotebook(updater: (current: NotebookStateV1) => NotebookStateV1) {
    setNotebook((current) => updater(current));
  }

  function updateBlock(id: string, updater: (draft: NotebookStateV1["blocks"][number]) => NotebookStateV1["blocks"][number]) {
    updateNotebook((current) => ({
      ...current,
      activeBlockId: id,
      blocks: current.blocks.map((block) => (block.id === id ? updater(block) : block)),
    }));
  }

  function setBlockRuntime(id: string, runtime: WorkbenchQueryState) {
    setRunStateById((current) => ({ ...current, [id]: runtime }));
  }

  async function runBlock(id: string) {
    const block = blockById.get(id);
    if (!block) return;

    const question = block.draft.trim();
    if (!question) return;

    updateNotebook((current) => ({ ...current, activeBlockId: id }));
    setBlockRuntime(id, { status: "loading" });

    const baseIr = {
      ...BASE_IR_BY_SIGNAL[block.signal],
      time_range: {
        from: String(BigInt(Math.floor(fromMs)) * 1_000_000n),
        to: String(BigInt(Math.floor(toMs)) * 1_000_000n),
      },
    };

    try {
      if (block.mode === "raw") {
        let rawIr: NlqIrLike;
        try {
          rawIr = JSON.parse(block.draft) as NlqIrLike;
        } catch {
          setBlockRuntime(id, {
            status: "error",
            message: "Raw mode expects valid JSON.",
          });
          return;
        }

        const mergedBaseIr: NlqIrLike = {
          ...baseIr,
          ...rawIr,
          operation: baseIr.operation,
          signals: baseIr.signals,
          time_range: baseIr.time_range,
        };

        const response = await submitNlqWithProvider(
          tenantId,
          { provider, webllmModel: config?.webllm_model },
          { base_ir: mergedBaseIr, mode: "execute" },
        );

        setBlockRuntime(
          id,
          response.type === "frame"
            ? { status: "result", response, question: block.draft }
            : { status: "result", response, question: block.draft },
        );
        return;
      }

      const response = await submitNlqWithProvider(
        tenantId,
        { provider, webllmModel: config?.webllm_model },
        { base_ir: baseIr, question, mode: "execute" },
      );
      setBlockRuntime(
        id,
        response.type === "frame"
          ? { status: "result", response, question }
          : response.type === "decline"
            ? { status: "result", response, question }
            : response.type === "invalid_response"
              ? { status: "result", response, question }
              : response.type === "capabilities"
                ? { status: "result", response, question }
                : { status: "result", response, question },
      );
    } catch (error) {
      setBlockRuntime(id, {
        status: "error",
        message: error instanceof Error ? error.message : "Query failed",
      });
    }
  }

  return (
    <div className="p-6 max-w-6xl mx-auto space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className="text-xl font-semibold">{notebook.title}</h1>
          <p className="text-sm text-[var(--text-muted)] mt-1">
            Shareable notebook workspace for metrics, logs, and traces.
          </p>
        </div>
        <div className="rounded border border-[var(--border)] bg-[var(--surface)] px-3 py-2 text-xs text-[var(--text-muted)]">
          URL state is synchronized as you edit blocks.
        </div>
      </div>

      <div className="grid gap-4 xl:grid-cols-3">
        {notebook.blocks.map((block) => (
          <NotebookBlock
            key={block.id}
            id={block.id}
            signal={block.signal}
            mode={block.mode}
            draft={block.draft}
            collapsed={block.collapsed}
            runtime={runStateById[block.id] ?? { status: "idle" }}
            isActive={notebook.activeBlockId === block.id}
            onActivate={() => {
              updateNotebook((current) => ({ ...current, activeBlockId: block.id }));
            }}
            onModeChange={(mode) => {
              updateBlock(block.id, (current) => ({ ...current, mode }));
            }}
            onDraftChange={(value) => {
              updateBlock(block.id, (current) => ({ ...current, draft: value }));
            }}
            onToggleCollapsed={() => {
              updateBlock(block.id, (current) => ({ ...current, collapsed: !current.collapsed }));
            }}
            onRun={() => void runBlock(block.id)}
          />
        ))}
      </div>

      <div className="flex flex-wrap gap-2 text-xs text-[var(--text-muted)]">
        <Button
          variant="secondary"
          className="min-h-8 px-2 text-xs"
          onClick={() => {
            setNotebook(createStarterWorkbenchState());
            setRunStateById(createIdleWorkbenchQueryStateMap(["metrics", "logs", "traces"]));
          }}
        >
          Reset notebook
        </Button>
        <span className="self-center">Current block: {notebook.activeBlockId}</span>
      </div>
    </div>
  );
}
