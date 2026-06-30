import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type MetricTone = "good" | "warn" | "bad" | "info";

export interface MetricCardProps extends HTMLAttributes<HTMLElement> {
  label: string;
  value: ReactNode;
  tone?: MetricTone;
  sparkline?: number[];
  delta?: number;
  deltaPositiveTone?: "good" | "bad";
}

const toneClasses: Record<MetricTone, string> = {
  good: "border-t-[var(--good)]",
  warn: "border-t-[var(--warn)]",
  bad: "border-t-[var(--bad)]",
  info: "border-t-[var(--brand)]",
};

function Sparkline({ values }: { values: number[] }) {
  if (values.length < 2) return null;

  const W = 100;
  const H = 32;
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1; // avoid div-by-zero for flat lines

  const points = values
    .map((v, i) => {
      const x = (i / (values.length - 1)) * W;
      const y = H - ((v - min) / range) * H;
      return `${x},${y}`;
    })
    .join(" ");

  return (
    <svg
      aria-hidden="true"
      width="100%"
      height={H}
      viewBox={`0 0 ${W} ${H}`}
      preserveAspectRatio="none"
      className="mt-2 block"
    >
      <polyline
        points={points}
        fill="none"
        stroke="var(--muted)"
        strokeWidth={1.5}
      />
    </svg>
  );
}

function DeltaBadge({
  delta,
  deltaPositiveTone = "bad",
}: {
  delta: number;
  deltaPositiveTone?: "good" | "bad";
}) {
  if (delta === 0) {
    return (
      <span className="mt-1 block text-xs text-[var(--muted)]">No change</span>
    );
  }

  const pct = (delta * 100).toFixed(1);
  const sign = delta > 0 ? "+" : "";
  const label = `${delta >= 0 ? "increased" : "decreased"} ${Math.abs(delta * 100).toFixed(1)} percent vs previous window`;

  // positive delta: color depends on whether higher = worse (bad) or better (good)
  const positiveColor =
    deltaPositiveTone === "bad" ? "var(--bad)" : "var(--good)";
  const negativeColor =
    deltaPositiveTone === "bad" ? "var(--good)" : "var(--bad)";
  const color = delta > 0 ? positiveColor : negativeColor;

  return (
    <span
      className="mt-1 flex items-baseline gap-1"
      aria-label={label}
    >
      <span className="text-xs font-semibold" style={{ color }}>
        {sign}{pct}%
      </span>
      <span className="text-xs text-[var(--muted)]">vs prev window</span>
    </span>
  );
}

export function MetricCard({
  label,
  value,
  tone = "info",
  sparkline,
  delta,
  deltaPositiveTone = "bad",
  className,
  ...props
}: MetricCardProps) {
  return (
    <article
      className={cn("modern-panel border-t-[3px] p-3", toneClasses[tone], className)}
      {...props}
    >
      <div className="metric-label">{label}</div>
      <div className="mt-1.5 text-2xl font-extrabold">{value}</div>
      {sparkline !== undefined && <Sparkline values={sparkline} />}
      {delta !== undefined && (
        <DeltaBadge delta={delta} deltaPositiveTone={deltaPositiveTone} />
      )}
    </article>
  );
}
