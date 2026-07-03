import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { QueryFilterInput } from "../../features/nlq/QueryFilterInput";
import type { NlqIrLike, QuerySurface } from "../../features/nlq/queryFilters";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
  lockedService?: boolean;
  showHeader?: boolean;
  showPromote?: boolean;
  querySurface?: Extract<QuerySurface, "logs" | "traces" | "metrics">;
  /**
   * Page base IR for NLQ filtering. When provided, `QueryFilterInput` uses the
   * new `baseIr`/`onSubmit` pattern and calls `onQuerySubmit` with the raw query text.
   */
  baseIr?: NlqIrLike;
  /**
   * Called with the raw query text when the user submits an NLQ query.
   * Required when `baseIr` is provided.
   */
  onQuerySubmit?: (text: string) => void;
  saveStatus: SaveStatus;
  onPromote: () => void;
  savedViewsControl?: ReactNode;
  histogram: ReactNode;
  renderTable: (selectedId: string | null, onSelect: (id: string | null) => void) => ReactNode;
  renderPanel: (selectedId: string, onClose: () => void) => ReactNode;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
}

export function SignalExplorer({
  title,
  service,
  onServiceChange,
  lockedService = false,
  showHeader = true,
  showPromote = true,
  baseIr,
  onQuerySubmit,
  saveStatus,
  onPromote,
  savedViewsControl,
  histogram,
  renderTable,
  renderPanel,
  selectedId: controlledId,
  onSelect: onControlledSelect,
}: SignalExplorerProps) {
  const [internalId, setInternalId] = useState<string | null>(null);
  const selectedId = controlledId !== undefined ? controlledId : internalId;

  function handleSelect(id: string | null) {
    const nextId = selectedId === id ? null : id;
    if (onControlledSelect) {
      onControlledSelect(nextId);
    } else {
      setInternalId(nextId);
    }
  }

  function handleServiceChange(s: string) {
    if (onControlledSelect) {
      onControlledSelect(null);
    } else {
      setInternalId(null);
    }
    onServiceChange(s);
  }

  function handleClear() {
    handleServiceChange("");
    onQuerySubmit?.("");
  }

  const hasActiveFilter = !!service;

  return (
    <div className="flex flex-col gap-3 h-full">
      {showHeader && (
        <div className="page-header">
          <div>
            <div className="text-xs font-bold uppercase text-[var(--muted)]">Explorer</div>
            <h1>{title}</h1>
          </div>
        </div>
      )}

      <div className="toolbar-row">
        {!lockedService && baseIr && onQuerySubmit && (
          <QueryFilterInput
            baseIr={baseIr}
            placeholder={`Filter ${title.toLowerCase()}, e.g. "checkout errors in prod"`}
            onSubmit={onQuerySubmit}
          />
        )}
        {hasActiveFilter && !lockedService && (
          <Button variant="secondary" onClick={handleClear}>
            Clear filters
          </Button>
        )}
        {savedViewsControl}
        {showPromote && (
          <>
            <Button variant="secondary" onClick={onPromote} disabled={saveStatus === "saving"}>
              Promote to dashboard
            </Button>
            {saveStatus === "saved" && (
              <span className="text-sm font-semibold text-[var(--good)]">Saved to dashboard</span>
            )}
            {saveStatus === "error" && (
              <span className="text-sm font-semibold text-[var(--bad)]">Dashboard save failed</span>
            )}
          </>
        )}
      </div>

      {histogram}

      <div className="flex flex-1 min-h-0 gap-3 overflow-hidden max-[900px]:flex-col">
        <div className="flex flex-1 min-h-0 min-w-0">
          {renderTable(selectedId, handleSelect)}
        </div>
        {selectedId !== null && (
          <div className="w-1/4 shrink-0 min-h-0 max-[900px]:w-full">
            {renderPanel(selectedId, () => handleSelect(null))}
          </div>
        )}
      </div>
    </div>
  );
}
