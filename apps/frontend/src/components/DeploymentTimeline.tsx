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

export function DeploymentTimeline({ markers, rangeStartMs, rangeEndMs }: Props) {
  const [tooltip, setTooltip] = useState<DeploymentMarker | null>(null);
  const width = 600;
  const height = 32;
  const lineHeight = 24;
  const lineY = 4;

  if (markers.length === 0) return null;

  return (
    <div className="deployment-timeline">
      <div className="field-label mb-1">Deployments</div>
      <svg
        width="100%"
        viewBox={`0 0 ${width} ${height}`}
        aria-label="Deployment timeline"
        className="deployment-timeline-svg"
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
                className="deployment-timeline-marker"
                onMouseEnter={() => setTooltip(m)}
                onMouseLeave={() => setTooltip(null)}
                aria-label={`Deployment ${m.service_version} — ${m.status}`}
              />
            </g>
          );
        })}
      </svg>

      {tooltip && (
        <div role="tooltip" className="deployment-timeline-tooltip">
          <div><strong>{tooltip.service_version}</strong></div>
          <div>{tooltip.status}</div>
          {tooltip.deployed_by && <div>by {tooltip.deployed_by}</div>}
          {tooltip.commit_sha && (
            <div className="deployment-timeline-commit">{tooltip.commit_sha.slice(0, 8)}</div>
          )}
        </div>
      )}
    </div>
  );
}
