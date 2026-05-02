import { type ReactNode, useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Select, SelectOption } from "../ui/select";

const timeRangeOptions = [
  { label: "15m", value: 15 },
  { label: "1h", value: 60 },
  { label: "6h", value: 360 },
  { label: "24h", value: 1440 },
];

export type SaveStatus = "idle" | "saving" | "saved" | "error";

export interface SignalExplorerProps {
  title: string;
  service: string;
  onServiceChange: (service: string) => void;
  lookbackMinutes: number;
  onLookbackChange: (minutes: number) => void;
  customRangeMs: { fromMs: number; toMs: number } | null;
  customRangeLabel?: string;
  onClearRange: () => void;
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
  lookbackMinutes,
  onLookbackChange,
  customRangeMs,
  customRangeLabel,
  onClearRange,
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

  function handleLookbackChange(m: number) {
    setSelectedId(null);
    onLookbackChange(m);
  }

  function handleClearRangeAndReset() {
    setSelectedId(null);
    onClearRange();
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
        {customRangeMs ? (
          <>
            {customRangeLabel && (
              <span className="text-xs whitespace-nowrap font-mono text-[var(--text-strong)]">
                {customRangeLabel}
              </span>
            )}
            <Button variant="secondary" onClick={handleClearRangeAndReset}>
              Reset range
            </Button>
          </>
        ) : (
          <Select
            aria-label={`${title} time range`}
            className="max-w-[120px]"
            value={String(lookbackMinutes)}
            onChange={(e) => handleLookbackChange(Number(e.target.value))}
          >
            {timeRangeOptions.map((opt) => (
              <SelectOption key={opt.value} value={opt.value}>
                {opt.label}
              </SelectOption>
            ))}
          </Select>
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
