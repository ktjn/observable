import { useState } from "react";
import { Button } from "../../../components/ui/button";

export interface ColumnDef<T extends string> {
  key: T;
  label: string;
}

export interface ColumnPickerControlProps<T extends string> {
  columns: ColumnDef<T>[];
  visibleColumns: T[];
  onChange: (columns: T[]) => void;
}

export function ColumnPickerControl<T extends string>({
  columns,
  visibleColumns,
  onChange,
}: ColumnPickerControlProps<T>) {
  const [isOpen, setIsOpen] = useState(false);

  function toggle(column: T) {
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
        <div className="absolute z-10 mt-1 w-48 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg">
          {columns.map(({ key, label }) => (
            <label key={key} className="flex items-center gap-2 py-1 text-sm">
              <input
                type="checkbox"
                checked={visibleColumns.includes(key)}
                onChange={() => toggle(key)}
              />
              {label}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}
