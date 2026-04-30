import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

type BadgeTone = "good" | "warn" | "bad" | "info" | "neutral";

export interface BadgeProps extends HTMLAttributes<HTMLSpanElement> {
  tone?: BadgeTone;
  children: ReactNode;
}

const toneClasses: Record<BadgeTone, string> = {
  good: "border border-[var(--good)] text-[var(--good)]",
  warn: "border border-[var(--warn)] text-[var(--warn)]",
  bad: "border border-[var(--bad)] text-[var(--bad)]",
  info: "border border-[var(--brand)] text-[var(--brand)]",
  neutral: "border border-[var(--border-strong)] text-[var(--muted)]",
};

export function Badge({ tone = "neutral", className, children, ...props }: BadgeProps) {
  return (
    <span
      role="status"
      className={cn(
        "inline-flex min-h-5 items-center px-1.5 text-[9px] font-bold uppercase tracking-wide",
        toneClasses[tone],
        className
      )}
      {...props}
    >
      {children}
    </span>
  );
}

type HealthState = "healthy" | "watch" | "breach" | "unknown";

const dotClasses: Record<HealthState, string> = {
  healthy: "bg-[var(--good)]",
  watch: "bg-[var(--warn)]",
  breach: "bg-[var(--bad)]",
  unknown: "bg-[var(--muted)]",
};

export function HealthDot({ state }: { state: HealthState }) {
  return (
    <span
      role="img"
      aria-label={state}
      className={cn("inline-block h-2 w-2 flex-shrink-0", dotClasses[state])}
    />
  );
}
