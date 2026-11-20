import { useId, useState } from "react";
import { GripVertical, ChevronUp, ChevronDown } from "lucide-react";
import { Button } from "../../../components/ui/button";

export interface ColumnDef<T extends string> {
  key: T;
  label: string;
}

export interface ColumnPickerControlProps<T extends string> {
  columns: ColumnDef<T>[];
  visibleColumns: T[];
  onToggle: (column: T) => void;
  onReorder: (order: T[]) => void;
}

export function ColumnPickerControl<T extends string>({
  columns,
  visibleColumns,
  onToggle,
  onReorder,
}: ColumnPickerControlProps<T>) {
  const [isOpen, setIsOpen] = useState(false);
  const [dragKey, setDragKey] = useState<T | null>(null);
  const listId = useId();

  function handleDrop(targetKey: T) {
    if (dragKey === null || dragKey === targetKey) {
      setDragKey(null);
      return;
    }
    const order = columns.map((c) => c.key);
    const fromIndex = order.indexOf(dragKey);
    const toIndex = order.indexOf(targetKey);
    order.splice(fromIndex, 1);
    order.splice(toIndex, 0, dragKey);
    onReorder(order);
    setDragKey(null);
  }

  function moveBy(key: T, delta: number) {
    const order = columns.map((c) => c.key);
    const fromIndex = order.indexOf(key);
    const toIndex = fromIndex + delta;
    if (toIndex < 0 || toIndex >= order.length) return;
    order.splice(fromIndex, 1);
    order.splice(toIndex, 0, key);
    onReorder(order);
  }

  return (
    <div className="relative">
      <Button
        variant="secondary"
        aria-expanded={isOpen}
        aria-haspopup="true"
        aria-controls={listId}
        onClick={() => setIsOpen((v) => !v)}
      >
        Columns
      </Button>
      {isOpen && (
        <div
          id={listId}
          className="absolute z-10 mt-1 w-48 border border-[var(--border)] bg-[var(--surface)] p-2 shadow-lg"
        >
          {columns.map(({ key, label }, index) => (
            <div
              key={key}
              draggable
              onDragStart={() => setDragKey(key)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={() => handleDrop(key)}
              onDragEnd={() => setDragKey(null)}
              className="flex items-center gap-1.5 py-1 text-sm"
            >
              <GripVertical
                className="size-3.5 shrink-0 cursor-grab text-[var(--muted)]"
                aria-hidden="true"
              />
              <label className="flex flex-1 items-center gap-2">
                <input type="checkbox" checked={visibleColumns.includes(key)} onChange={() => onToggle(key)} />
                {label}
              </label>
              <button
                type="button"
                className="shrink-0 text-[var(--muted)] hover:text-[var(--text)] disabled:opacity-30"
                disabled={index === 0}
                aria-label={`Move ${label} up`}
                onClick={() => moveBy(key, -1)}
              >
                <ChevronUp className="size-3.5" aria-hidden="true" />
              </button>
              <button
                type="button"
                className="shrink-0 text-[var(--muted)] hover:text-[var(--text)] disabled:opacity-30"
                disabled={index === columns.length - 1}
                aria-label={`Move ${label} down`}
                onClick={() => moveBy(key, 1)}
              >
                <ChevronDown className="size-3.5" aria-hidden="true" />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
