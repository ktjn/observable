import { useEffect, useRef, useState } from "react";
import type { DeploymentMarker } from "../../api/deployments";
import { markerColor, markerPosition } from "../DeploymentTimeline";

export interface TimeSeriesPoint {
  timestampMs: number;
  value: number;
}

export interface TimeSeriesSeries {
  key: string;
  label: string;
  color: string;
  dashed?: boolean;
  formatY?: (value: number) => string;
  points: TimeSeriesPoint[];
}

export interface TimeSeriesGraphProps {
  series: TimeSeriesSeries[];
  deploymentMarkers?: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
  height?: number;
  title?: string;
  eyebrow?: string;
  ariaLabel?: string;
}

const PLOT_TOP = 10;
const AXIS_HEIGHT = 18;
const PLOT_BOTTOM_MARGIN = 6;

export function toX(
  timestampMs: number,
  rangeStartMs: number,
  rangeEndMs: number,
  width: number,
): number {
  const span = rangeEndMs - rangeStartMs;
  if (span <= 0) return 0;
  return Math.round(((timestampMs - rangeStartMs) / span) * width);
}

export function toY(
  value: number,
  min: number,
  max: number,
  plotTop: number,
  plotBottom: number,
): number {
  const range = max - min;
  if (range === 0) return Math.round((plotTop + plotBottom) / 2);
  const ratio = (value - min) / range;
  return Math.round(plotBottom - ratio * (plotBottom - plotTop));
}

export function buildPolylinePoints(
  series: TimeSeriesSeries,
  rangeStartMs: number,
  rangeEndMs: number,
  width: number,
  plotTop: number,
  plotBottom: number,
): string {
  if (series.points.length === 0) return "";
  const values = series.points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  return series.points
    .map(
      (p) =>
        `${toX(p.timestampMs, rangeStartMs, rangeEndMs, width)},${toY(p.value, min, max, plotTop, plotBottom)}`,
    )
    .join(" ");
}

export function TimeSeriesGraph({
  series,
  deploymentMarkers = [],
  rangeStartMs,
  rangeEndMs,
  height = 80,
  title,
  eyebrow,
  ariaLabel = "Time series graph",
}: TimeSeriesGraphProps) {
  const wrapperRef = useRef<HTMLElement>(null);
  const [width, setWidth] = useState(400);
  const [hoverX, setHoverX] = useState<number | null>(null);
  const [deployTooltip, setDeployTooltip] = useState<DeploymentMarker | null>(null);

  useEffect(() => {
    const el = wrapperRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      setWidth(Math.round(entry.contentRect.width));
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const plotBottom = height - AXIS_HEIGHT - PLOT_BOTTOM_MARGIN;
  const gridYs = [0.25, 0.5, 0.75].map((r) =>
    Math.round(PLOT_TOP + r * (plotBottom - PLOT_TOP)),
  );

  const timeSteps = [0, 0.25, 0.5, 0.75, 1];
  const timeLabels = timeSteps.map((ratio) => ({
    x: Math.round(ratio * width),
    label: formatTimeLabel(rangeStartMs + ratio * (rangeEndMs - rangeStartMs)),
    anchor: ratio === 0 ? "start" : ratio === 1 ? "end" : "middle",
  }));

  const hoverTimestampMs =
    hoverX != null
      ? rangeStartMs + (hoverX / width) * (rangeEndMs - rangeStartMs)
      : null;

  function nearestPoint(s: TimeSeriesSeries): TimeSeriesPoint | null {
    if (hoverTimestampMs == null || s.points.length === 0) return null;
    return s.points.reduce((a, b) =>
      Math.abs(a.timestampMs - hoverTimestampMs) <=
      Math.abs(b.timestampMs - hoverTimestampMs)
        ? a
        : b,
    );
  }

  return (
    <section
      ref={wrapperRef}
      role="group"
      aria-label={ariaLabel}
      className="border border-[var(--border)] bg-[var(--surface)] p-3"
    >
      {(eyebrow || title || series.length > 0) && (
        <div className="mb-2 flex items-center justify-between gap-3">
          <div>
            {eyebrow && (
              <div className="text-xs font-bold uppercase text-[var(--muted)]">{eyebrow}</div>
            )}
            {title && (
              <h2 className="m-0 text-sm font-bold text-[var(--text-strong)]">{title}</h2>
            )}
          </div>
          <div className="flex flex-wrap gap-3 text-xs text-[var(--muted)]">
            {series.map((s) => (
              <span key={s.key} className="inline-flex items-center gap-1">
                <svg width="12" height="4" aria-hidden="true">
                  <line
                    x1="0" y1="2" x2="12" y2="2"
                    stroke={s.color}
                    strokeWidth="2"
                    strokeDasharray={s.dashed ? "3 2" : undefined}
                  />
                </svg>
                {s.label}
              </span>
            ))}
          </div>
        </div>
      )}

      <div className="relative" style={{ height }}>
        <svg
          width="100%"
          height={height}
          viewBox={`0 0 ${width} ${height}`}
          aria-hidden="true"
          onPointerMove={(e) => {
            const rect = e.currentTarget.getBoundingClientRect();
            setHoverX(Math.round(e.clientX - rect.left));
          }}
          onPointerLeave={() => setHoverX(null)}
          style={{ cursor: "crosshair", overflow: "visible" }}
        >
          {gridYs.map((y) => (
            <line
              key={y}
              x1={0} y1={y} x2={width} y2={y}
              stroke="var(--border)"
              strokeWidth={0.5}
            />
          ))}

          {series.map((s) => (
            <polyline
              key={s.key}
              points={buildPolylinePoints(s, rangeStartMs, rangeEndMs, width, PLOT_TOP, plotBottom)}
              fill="none"
              stroke={s.color}
              strokeWidth={1.5}
              strokeDasharray={s.dashed ? "4 2" : undefined}
            />
          ))}

          {deploymentMarkers.map((m) => {
            const x = markerPosition(
              new Date(m.started_at).getTime(),
              rangeStartMs,
              rangeEndMs,
              width,
            );
            const color = markerColor(m.status);
            return (
              <g key={m.deployment_id}>
                <line
                  x1={x} y1={PLOT_TOP}
                  x2={x} y2={plotBottom}
                  stroke={color}
                  strokeWidth={1}
                  strokeDasharray="3 2"
                  opacity={0.7}
                />
                <polygon
                  points={`${x},${PLOT_TOP - 2} ${x - 4},${PLOT_TOP + 5} ${x + 4},${PLOT_TOP + 5}`}
                  fill={color}
                  onMouseEnter={() => setDeployTooltip(m)}
                  onMouseLeave={() => setDeployTooltip(null)}
                  style={{ cursor: "default" }}
                  aria-label={`Deployment ${m.service_version} — ${m.status}`}
                />
              </g>
            );
          })}

          {hoverX != null && (
            <line
              x1={hoverX} y1={PLOT_TOP}
              x2={hoverX} y2={plotBottom}
              stroke="var(--muted)"
              strokeWidth={1}
              strokeDasharray="2 2"
              opacity={0.4}
            />
          )}

          {timeLabels.map(({ x, label, anchor }) => (
            <text
              key={x}
              x={x}
              y={height - 2}
              fontSize={9}
              fill="var(--muted)"
              textAnchor={anchor as "start" | "middle" | "end"}
            >
              {label}
            </text>
          ))}
        </svg>

        {hoverX != null && (
          <div
            className="pointer-events-none absolute z-10 min-w-[110px] border border-[var(--border)] bg-[var(--surface)] p-2 text-xs shadow-md"
            style={{ left: hoverX + 10, top: PLOT_TOP }}
          >
            {series.map((s) => {
              const pt = nearestPoint(s);
              if (!pt) return null;
              return (
                <div key={s.key} className="flex items-center gap-1.5">
                  <span
                    className="inline-block h-px w-2.5 shrink-0"
                    style={{ background: s.color }}
                  />
                  <span className="text-[var(--muted)]">{s.label}:</span>
                  <span className="font-mono font-bold text-[var(--text-strong)]">
                    {s.formatY ? s.formatY(pt.value) : pt.value.toFixed(1)}
                  </span>
                </div>
              );
            })}
          </div>
        )}

        {deployTooltip && (
          <div role="tooltip" className="deployment-timeline-tooltip">
            <div><strong>{deployTooltip.service_version}</strong></div>
            <div>{deployTooltip.status}</div>
            {deployTooltip.deployed_by && <div>by {deployTooltip.deployed_by}</div>}
            {deployTooltip.commit_sha && (
              <div className="deployment-timeline-commit">
                {deployTooltip.commit_sha.slice(0, 8)}
              </div>
            )}
          </div>
        )}
      </div>
    </section>
  );
}

function formatTimeLabel(ms: number): string {
  const d = new Date(ms);
  return `${d.getHours().toString().padStart(2, "0")}:${d.getMinutes().toString().padStart(2, "0")}`;
}
