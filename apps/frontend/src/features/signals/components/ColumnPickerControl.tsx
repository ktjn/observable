import { useState } from "react";
import { Button } from "../../../components/ui/button";
import type { LogTableColumn } from "./LogResultsTable";

const COLUMN_LABELS: Record<LogTableColumn, string> = {
  level: "Level",
  service: "Service",
};

const ALL_COLUMNS: LogTableColumn[] = ["level", "service"];

export interface ColumnPickerControlProps {
  visibleColumns: LogTableColumn[];
  onChange: (columns: LogTableColumn[]) => void;
}

export function ColumnPickerControl({ visibleColumns, onChange }: ColumnPickerControlProps) {
  const [isOpen, setIsOpen] = useState(false);

  function toggle(column: LogTableColumn) {
    if (visibleColumns.includes(column)) {
      onChange(visibleColumns.filter((c) => c !== column));
    } else {
      onChange([...visibleColumns, column]);
    }
  }

  return (
    <div className="relative">
      <Button variant="secondary" onClick={() => setIsOpen((v) => !v)}>
        Columns
      </Button>
      {isOpen && (
        <div className="absolute z-10 mt-1 w-40 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          {ALL_COLUMNS.map((column) => (
            <label key={column} className="flex items-center gap-2 py-1 text-sm">
              <input
                type="checkbox"
                checked={visibleColumns.includes(column)}
                onChange={() => toggle(column)}
              />
              {COLUMN_LABELS[column]}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
