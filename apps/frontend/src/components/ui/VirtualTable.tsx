import { useRef } from "react";
import type { ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

interface VirtualTableProps<T> {
  rows: T[];
  renderHead: () => ReactNode;
  renderRow: (row: T, ref: (el: Element | null) => void, index: number) => ReactNode;
  estimateSize?: number;
  /** Must be a fixed CSS value for virtualisation to work (e.g. `"600px"`, not `"auto"`). */
  height?: string;
  ariaLabel?: string;
}

export function VirtualTable<T>({
  rows,
  renderHead,
  renderRow,
  estimateSize = 40,
  height = "600px",
  ariaLabel,
}: VirtualTableProps<T>) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => estimateSize,
    measureElement: (el) => el?.getBoundingClientRect().height ?? estimateSize,
  });

  const virtualItems = virtualizer.getVirtualItems();
  const totalSize = virtualizer.getTotalSize();
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0;
  const paddingBottom =
    virtualItems.length > 0 ? totalSize - virtualItems[virtualItems.length - 1].end : 0;

  return (
    <div ref={parentRef} style={{ height, overflowY: "auto" }}>
      <table aria-label={ariaLabel}>
        <thead className="sticky top-0 z-10 bg-[var(--surface)]">{renderHead()}</thead>
        <tbody>
          {paddingTop > 0 && (
            <tr aria-hidden="true">
              <td colSpan={999} style={{ height: paddingTop, padding: 0, border: 0 }} />
            </tr>
          )}
          {virtualItems.map((virtualRow) =>
            renderRow(rows[virtualRow.index], virtualizer.measureElement, virtualRow.index),
          )}
          {paddingBottom > 0 && (
            <tr aria-hidden="true">
              <td colSpan={999} style={{ height: paddingBottom, padding: 0, border: 0 }} />
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
