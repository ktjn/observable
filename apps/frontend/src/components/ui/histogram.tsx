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

const PLOT_HEIGHT = 96;
const GAP_PX = 2;
const X_AXIS_HEIGHT = 18;

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

  // Compute x-axis tick positions: first, last, and 1-3 evenly-spaced middle buckets
  const xTicks: { index: number; ms: number }[] = (() => {
    if (buckets.length === 0) return [];
    if (buckets.length === 1) return [{ index: 0, ms: buckets[0].startMs }];
    const last = buckets.length - 1;
    const result: { index: number; ms: number }[] = [{ index: 0, ms: buckets[0].startMs }];
    // Add 1-3 middle ticks (targeting ~3-5 total ticks)
    const numMiddle = Math.min(3, buckets.length - 2);
    for (let m = 1; m <= numMiddle; m++) {
      const idx = Math.round((m * last) / (numMiddle + 1));
      if (idx > 0 && idx < last) {
        result.push({ index: idx, ms: buckets[idx].startMs });
      }
    }
    result.push({ index: last, ms: buckets[last].startMs });
    return result;
  })();
  const sectionRef = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(400);

  const onBucketCountChangeRef = useRef(onBucketCountChange);
  useEffect(() => {
    onBucketCountChangeRef.current = onBucketCountChange;
  });

  useEffect(() => {
    const el = sectionRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const w = entry.contentRect.width;
      setWidth(Math.round(w));
      const count = Math.round(Math.floor(w / 10) / 5) * 5;
      onBucketCountChangeRef.current?.(Math.max(12, Math.min(100, count)));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const dragRef = useRef<{ startX: number; endX: number; rectWidth: number } | null>(null);
  const [dragDisplay, setDragDisplay] = useState<{ startX: number; endX: number } | null>(null);

  const barWidth = buckets.length > 0 ? width / buckets.length : 0;

  function xToMs(x: number, rectWidth: number): number {
    const bw = buckets.length > 0 ? rectWidth / buckets.length : 0;
    if (buckets.length === 0 || bw <= 0) return buckets[0]?.startMs ?? 0;
    const idx = Math.min(buckets.length - 1, Math.max(0, Math.floor(x / bw)));
    return buckets[idx].startMs;
  }

  function handlePointerDown(e: React.PointerEvent<SVGSVGElement>) {
    if (!onRangeSelect) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    try {
      e.currentTarget.setPointerCapture(e.pointerId);
    } catch {
      /* jsdom */
    }
    dragRef.current = { startX: x, endX: x, rectWidth: rect.width };
    setDragDisplay({ startX: x, endX: x });
  }

  function handlePointerMove(e: React.PointerEvent<SVGSVGElement>) {
    if (!dragRef.current) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = e.clientX - rect.left;
    dragRef.current = { ...dragRef.current, endX: x };
    setDragDisplay({ startX: dragRef.current.startX, endX: x });
  }

  function handlePointerUp() {
    const drag = dragRef.current;
    if (drag && onRangeSelect && buckets.length > 0) {
      const fromMs = xToMs(Math.min(drag.startX, drag.endX), drag.rectWidth);
      const toMs =
        xToMs(Math.max(drag.startX, drag.endX), drag.rectWidth) +
        (buckets[1]?.startMs ?? buckets[0].endMs) -
        buckets[0].startMs;
      onRangeSelect(fromMs, toMs);
    }
    dragRef.current = null;
    setDragDisplay(null);
  }

  const selStartX = dragDisplay ? Math.min(dragDisplay.startX, dragDisplay.endX) : -1;
  const selEndX = dragDisplay ? Math.max(dragDisplay.startX, dragDisplay.endX) : -1;

  return (
    <section
      ref={sectionRef}
      role="group"
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
      <svg
        width="100%"
        height={PLOT_HEIGHT + X_AXIS_HEIGHT}
        viewBox={`0 0 ${width} ${PLOT_HEIGHT + X_AXIS_HEIGHT}`}
        aria-hidden="true"
        className="select-none"
        style={{ cursor: onRangeSelect ? "crosshair" : "default" }}
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onPointerUp={handlePointerUp}
        onPointerCancel={() => {
          dragRef.current = null;
          setDragDisplay(null);
        }}
      >
        {[0.25, 0.5, 0.75].map((r) => (
          <line
            key={r}
            x1={0}
            y1={PLOT_HEIGHT * r}
            x2={width}
            y2={PLOT_HEIGHT * r}
            stroke="var(--border)"
            strokeWidth={0.5}
          />
        ))}

        {buckets.map((bucket, i) => {
          const x = i * barWidth;
          const isSelected = dragDisplay !== null && x + barWidth / 2 >= selStartX && x + barWidth / 2 <= selEndX;
          let stackedHeight = 0;
          return (
            <g key={bucket.startMs}>
              <rect
                x={x}
                y={0}
                width={Math.max(0, barWidth - GAP_PX)}
                height={PLOT_HEIGHT}
                fill={isSelected ? "var(--surface-subtle)" : "var(--surface-inset)"}
              />
              {categoryOrder.map((cat) => {
                const count = bucket.categories[cat];
                if (count === 0) return null;
                const segHeight = Math.max(2, (count / max) * PLOT_HEIGHT);
                const y = PLOT_HEIGHT - stackedHeight - segHeight;
                stackedHeight += segHeight;
                const segmentProps = {
                  x,
                  y,
                  width: Math.max(0, barWidth - GAP_PX),
                  height: segHeight,
                  className: categoryColors[cat],
                  title: `${format(bucket.startMs)} ${cat}: ${count}`,
                } satisfies React.SVGProps<SVGRectElement> & { title: string };
                return <rect key={cat} {...segmentProps} />;
              })}
            </g>
          );
        })}
        {/* Y-axis max label */}
        {buckets.length > 0 && max > 1 && (
          <text
            x={4}
            y={10}
            fontSize={10}
            fill="var(--muted)"
            textAnchor="start"
          >
            {max}
          </text>
        )}

        {/* X-axis time tick labels */}
        {xTicks.map(({ index, ms }, tickIdx) => {
          const isFirst = tickIdx === 0;
          const isLast = tickIdx === xTicks.length - 1;
          const anchor = isFirst ? "start" : isLast ? "end" : "middle";
          const x = isFirst ? 0 : isLast ? width : (index + 0.5) * barWidth;
          return (
            <text
              key={ms}
              x={x}
              y={PLOT_HEIGHT + 13}
              fontSize={10}
              fill="var(--muted)"
              textAnchor={anchor}
            >
              {format(ms)}
            </text>
          );
        })}
      </svg>
    </section>
  );
}
