import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type MetricTone = "good" | "warn" | "bad" | "info";

export interface MetricCardProps extends HTMLAttributes<HTMLDivElement> {
  label: string;
  value: ReactNode;
  tone?: MetricTone;
}

const toneClasses: Record<MetricTone, string> = {
  good: "border-t-[var(--good)]",
  warn: "border-t-[var(--warn)]",
  bad: "border-t-[var(--bad)]",
  info: "border-t-[var(--brand)]",
};

export function MetricCard({
  label,
  value,
  tone = "info",
  className,
  ...props
}: MetricCardProps) {
  return (
    <div
      className={cn("modern-panel border-t-[3px] p-3", toneClasses[tone], className)}
      {...props}
    >
      <div className="metric-label">{label}</div>
      <div className="mt-1.5 text-2xl font-extrabold">{value}</div>
    </div>
  );
}
