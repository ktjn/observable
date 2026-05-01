import { useEffect, useRef, useState } from "react";

export type HistogramBucket<T extends string = string> = {
  startMs: number;
  endMs: number;
  total: number;
  categories: Record<T, number>;
};

export interface HistogramProps<T extends string> {
  buckets: HistogramBucket<T>[];
  categoryOrder: T[];
  categoryColors: Record<T, string>;
  format: (ms: number) => string;
  onRangeSelect?: (fromMs: number, toMs: number) => void;
  onBucketCountChange?: (count: number) => void;
  ariaLabel?: string;
  title?: string;
  subtitle?: string;
}

export function Histogram<T extends string>({
  buckets,
  categoryOrder,
  categoryColors,
  format,
  onRangeSelect,
  onBucketCountChange,
  ariaLabel = "Data volume histogram",
  title,
  subtitle,
}: HistogramProps<T>) {
  const max = Math.max(1, ...buckets.map((bucket) => bucket.total));
  const gridRef = useRef<HTMLDivElement>(null);
  const sectionRef = useRef<HTMLElement>(null);

  const onBucketCountChangeRef = useRef(onBucketCountChange);
  useEffect(() => {
    onBucketCountChangeRef.current = onBucketCountChange;
  });

  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      const count = Math.round(Math.floor(w / 10) / 5) * 5;
      onBucketCountChangeRef.current?.(Math.max(12, Math.min(100, count)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const dragRef = useRef<{ start: number; end: number } | null>(null);
  const [dragDisplay, setDragDisplay] = useState<{ start: number; end: number } | null>(null);

  const selStart = dragDisplay ? Math.min(dragDisplay.start, dragDisplay.end) : -1;
  const selEnd = dragDisplay ? Math.max(dragDisplay.start, dragDisplay.end) : -1;

  function getBucketIndex(clientX: number): number {
    const el = gridRef.current;
    if (!el) return 0;
    const rect = el.getBoundingClientRect();
    const ratio = (clientX - rect.left) / rect.width;
    return Math.min(buckets.length - 1, Math.max(0, Math.floor(ratio * buckets.length)));
  }

  function handlePointerDown(e: React.PointerEvent<HTMLDivElement>) {
    if (!onRangeSelect) return;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* jsdom */
    }
    const idx = getBucketIndex(e.clientX);
    dragRef.current = { start: idx, end: idx };
    setDragDisplay({ start: idx, end: idx });
  }

  function handlePointerMove(e: React.PointerEvent<HTMLDivElement>) {
    if (!dragRef.current) return;
    const idx = getBucketIndex(e.clientX);
    dragRef.current = { ...dragRef.current, end: idx };
    setDragDisplay({ ...dragRef.current });
  }

  function handlePointerUp() {
    const drag = dragRef.current;
    if (drag && onRangeSelect) {
      const start = Math.min(drag.start, drag.end);
      const end = Math.max(drag.start, drag.end);
      onRangeSelect(buckets[start].startMs, buckets[end].endMs);
    }
    dragRef.current = null;
    setDragDisplay(null);
  }

  return (
    <section
      ref={sectionRef}
      role="img"
      aria-label={ariaLabel}
      className="border border-[var(--border)] bg-[var(--surface)] p-3"
    >
      {(title || subtitle || categoryOrder.length > 0) && (
        <div className="mb-3 flex items-center justify-between gap-3">
          <div>
            {subtitle && <div className="text-xs font-bold uppercase text-[var(--muted)]">{subtitle}</div>}
            {title && <h2 className="m-0 text-sm font-bold text-[var(--text-strong)]">{title}</h2>}
          </div>
          <div className="flex flex-wrap gap-2 text-xs text-[var(--muted)]">
            {categoryOrder.map((cat) => (
              <span key={cat} className="inline-flex items-center gap-1">
                <span className={`h-2 w-2 ${categoryColors[cat]}`} />
                {cat}
              </span>
            ))}
          </div>
        </div>
      )}
      <p className="sr-only">Drag over bars to zoom into a time range.</p>
      <div
        ref={gridRef}
        className="grid h-28 items-end gap-1 select-none cursor-crosshair"
        style={{ gridTemplateColumns: `repeat(${buckets.length}, 1fr)` }}
        aria-hidden="true"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={() => {
          dragRef.current = null;
          setDragDisplay(null);
        }}
      >
        {buckets.map((bucket, i) => {
          const isSelected = dragDisplay !== null && i >= selStart && i <= selEnd;
          return (
            <div
              key={bucket.startMs}
              className={`flex h-full flex-col justify-end gap-px ${
                isSelected ? "bg-[var(--surface-subtle)]" : "bg-[var(--surface-inset)]"
              }`}
            >
              {categoryOrder.map((cat) => {
                const count = bucket.categories[cat];
                if (count === 0) return null;
                return (
                  <div
                    key={cat}
                    className={categoryColors[cat]}
                    title={`${format(bucket.startMs)} ${cat}: ${count}`}
                    style={{ height: `${Math.max(8, (count / max) * 100)}%` }}
                  />
                );
              })}
            </div>
          );
        })}
      </div>
    </section>
  );
}
