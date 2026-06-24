import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";

const PAGE_SIZE = 50;

interface VirtualTableProps<T> {
  rows: T[];
  renderHead: () => ReactNode;
  renderRow: (row: T, ref: (el: Element | null) => void, index: number) => ReactNode;
  estimateSize?: number;
  ariaLabel?: string;
}

export function VirtualTable<T>({
  rows,
  renderHead,
  renderRow,
  ariaLabel,
}: VirtualTableProps<T>) {
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);
  const sentinelRef = useRef<HTMLTableRowElement>(null);

  // Reset page when the row set changes (new query / filter)
  useEffect(() => {
    setVisibleCount(PAGE_SIZE);
  }, [rows]);

  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisibleCount((n) => Math.min(n + PAGE_SIZE, rows.length));
        }
      },
      { threshold: 0.1 },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [rows.length]);

  const visibleRows = rows.slice(0, visibleCount);

  return (
    <div style={{ flex: 1, minWidth: 0, minHeight: 0, overflow: "auto" }}>
      <table aria-label={ariaLabel}>
        <thead className="sticky top-0 z-10 bg-[var(--surface)]">{renderHead()}</thead>
        <tbody>
          {visibleRows.map((row, index) => renderRow(row, () => {}, index))}
          {visibleCount < rows.length && (
            <tr ref={sentinelRef} aria-hidden="true">
              <td colSpan={999} style={{ height: 40, padding: 0, border: 0 }} />
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
