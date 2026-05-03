import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { QueryFilterInput } from "../../features/nlq/QueryFilterInput";
import type { NlqIrLike } from "../../features/nlq/queryFilters";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
  lockedService?: boolean;
  showHeader?: boolean;
  showPromote?: boolean;
  querySurface?: Extract<QuerySurface, "logs" | "traces" | "metrics">;
  saveStatus: SaveStatus;
  onPromote: () => void;
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
    <div className="page-stack">
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
            placeholder={`Filter ${title.toLowerCase()} with NLQ or raw NLQ IR JSON`}
            onSubmit={onQuerySubmit}
          />
        )}
        {hasActiveFilter && !lockedService && (
          <Button variant="secondary" onClick={handleClear}>
            Clear filters
          </Button>
        )}
        {showPromote && (
          <>
            <Button onClick={onPromote} disabled={saveStatus === "saving"}>
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

      <div className="flex items-start gap-3 max-[900px]:flex-col">
        <div className="flex flex-1 items-start gap-3">
          {renderTable(selectedId, handleSelect)}
        </div>
        {selectedId !== null && (
          <div className="w-1/4 shrink-0">
            {renderPanel(selectedId, () => handleSelect(null))}
          </div>
        )}
      </div>
    </div>
  );
}
