import { useState } from "react";
import type { DeploymentMarker } from "../api/deployments";

export function markerColor(status: string): string {
  switch (status) {
    case "success":     return "#22c55e";
    case "in_progress": return "#3b82f6";
    case "failed":      return "#ef4444";
    case "rolled_back": return "#f97316";
    default:            return "#9ca3af";
  }
}

export function markerPosition(
  timestampMs: number,
  rangeStartMs: number,
  rangeEndMs: number,
  widthPx: number,
): number {
  const span = rangeEndMs - rangeStartMs;
  if (span <= 0) return 0;
  const ratio = (timestampMs - rangeStartMs) / span;
  return Math.round(Math.min(Math.max(ratio, 0), 1) * widthPx);
}

interface Props {
  markers: DeploymentMarker[];
  rangeStartMs: number;
  rangeEndMs: number;
}

interface TooltipState {
  marker: DeploymentMarker;
  x: number;
  y: number;
}

export function DeploymentTimeline({ markers, rangeStartMs, rangeEndMs }: Props) {
  const [tooltip, setTooltip] = useState<TooltipState | null>(null);
  const width = 600;
  const height = 32;
  const lineHeight = 24;
  const lineY = 4;

  if (markers.length === 0) return null;

  return (
    <div className="deployment-timeline" style={{ position: "relative" }}>
      <div className="field-label" style={{ marginBottom: "4px" }}>Deployments</div>
      <svg
        width="100%"
        viewBox={`0 0 ${width} ${height}`}
        aria-label="Deployment timeline"
        style={{ display: "block" }}
      >
        <line
          x1={0} y1={lineY + lineHeight / 2}
          x2={width} y2={lineY + lineHeight / 2}
          stroke="#374151" strokeWidth={1}
        />
        {markers.map((m) => {
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
                x1={x} y1={lineY} x2={x} y2={lineY + lineHeight}
                stroke={color} strokeWidth={2}
              />
              <circle
                cx={x} cy={lineY + lineHeight / 2} r={5}
                fill={color}
                style={{ cursor: "pointer" }}
                onMouseEnter={(e) => {
                  const svgRect = (e.currentTarget.closest("svg") as SVGElement)
                    .getBoundingClientRect();
                  setTooltip({ marker: m, x: e.clientX - svgRect.left, y: e.clientY - svgRect.top });
                }}
                onMouseLeave={() => setTooltip(null)}
                aria-label={`Deployment ${m.service_version} — ${m.status}`}
              />
            </g>
          );
        })}
      </svg>

      {tooltip && (
        <div
          role="tooltip"
          style={{
            position: "absolute",
            left: tooltip.x + 8,
            top: tooltip.y - 8,
            background: "#1f2937",
            border: "1px solid #374151",
            borderRadius: "4px",
            padding: "6px 10px",
            fontSize: "12px",
            color: "#f3f4f6",
            pointerEvents: "none",
            zIndex: 10,
            whiteSpace: "nowrap",
          }}
        >
          <div><strong>{tooltip.marker.service_version}</strong></div>
          <div style={{ color: markerColor(tooltip.marker.status) }}>{tooltip.marker.status}</div>
          {tooltip.marker.deployed_by && <div>by {tooltip.marker.deployed_by}</div>}
          {tooltip.marker.commit_sha && (
            <div style={{ fontFamily: "monospace" }}>{tooltip.marker.commit_sha.slice(0, 8)}</div>
          )}
        </div>
      )}
    </div>
  );
}
