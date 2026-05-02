import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
  lockedService?: boolean;
  showHeader?: boolean;
  showPromote?: boolean;
  saveStatus: SaveStatus;
  onPromote: () => void;
  histogram: ReactNode;
  renderTable: (selectedId: string | null, onSelect: (id: string | null) => void) => ReactNode;
  renderPanel: (selectedId: string, onClose: () => void) => ReactNode;
}

export function SignalExplorer({
  title,
  service,
  onServiceChange,
  lockedService = false,
  showHeader = true,
  showPromote = true,
  saveStatus,
  onPromote,
  histogram,
  renderTable,
  renderPanel,
}: SignalExplorerProps) {
  const [selectedId, setSelectedId] = useState<string | null>(null);

  function handleSelect(id: string | null) {
    setSelectedId((prev) => (prev === id ? null : id));
  }

  function handleServiceChange(s: string) {
    setSelectedId(null);
    onServiceChange(s);
  }

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
        {!lockedService && (
          <Input
            className="max-w-[360px]"
            placeholder="Filter by service"
            value={service}
            onChange={(e) => handleServiceChange(e.target.value)}
            aria-label="Filter by service"
          />
        )}
        {service && !lockedService && (
          <Button variant="secondary" onClick={() => handleServiceChange("")}>
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
            {renderPanel(selectedId, () => setSelectedId(null))}
          </div>
        )}
      </div>
    </div>
  );
}
