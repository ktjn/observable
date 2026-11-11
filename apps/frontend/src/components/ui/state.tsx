import type { ReactNode } from "react";
import { cn } from "./cn";

/**
 * Shared state vocabulary (phase 2 E7, OBS-24-01, OBS-24-02, OBS-13-03).
 *
 * Every state is told apart by text and shape as well as colour, so it survives grayscale and
 * reads the same to a screen reader. `unknown` never borrows the look of a healthy state.
 */

export type ServiceState = "normal" | "degraded" | "unavailable" | "planned_maintenance" | "unknown";

type Shape = "circle" | "triangle" | "square" | "diamond" | "ring";

const states: Record<ServiceState, { label: string; shape: Shape; tone: string }> = {
  normal: { label: "Normal", shape: "circle", tone: "text-[var(--good)]" },
  degraded: { label: "Degraded", shape: "triangle", tone: "text-[var(--warn)]" },
  unavailable: { label: "Unavailable", shape: "square", tone: "text-[var(--bad)]" },
  planned_maintenance: { label: "Planned maintenance", shape: "diamond", tone: "text-[var(--brand)]" },
  unknown: { label: "Unknown", shape: "ring", tone: "text-[var(--muted)]" },
};

export const stateLabels: Record<ServiceState, string> = Object.fromEntries(
  Object.entries(states).map(([state, { label }]) => [state, label]),
) as Record<ServiceState, string>;

function StateIcon({ shape }: { shape: Shape }) {
  const common = { fill: "currentColor", stroke: "currentColor", strokeWidth: 1.5 };
  return (
    <svg aria-hidden="true" focusable="false" viewBox="0 0 12 12" className="h-3 w-3 flex-shrink-0" data-shape={shape}>
      {shape === "circle" && <circle cx="6" cy="6" r="5" {...common} />}
      {shape === "triangle" && <path d="M6 1 11 11H1Z" {...common} />}
      {shape === "square" && <rect x="1.5" y="1.5" width="9" height="9" {...common} />}
      {shape === "diamond" && <path d="M6 0.8 11.2 6 6 11.2 0.8 6Z" {...common} />}
      {shape === "ring" && <circle cx="6" cy="6" r="4.5" fill="none" stroke="currentColor" strokeWidth={1.5} strokeDasharray="2 1.5" />}
    </svg>
  );
}

/** A service state: icon shape, visible text and an accessible name that includes the reason. */
export function StateLabel({ state, reason, subject = "Health" }: { state: ServiceState; reason?: string; subject?: string }) {
  const { label, shape, tone } = states[state] ?? states.unknown;
  const name = `${subject}: ${label}${reason ? `, ${reason}` : ""}`;
  return (
    <span role="img" aria-label={name} data-state={state}
      className={cn("inline-flex items-center gap-1 text-xs font-semibold", tone)}>
      <StateIcon shape={shape} />
      <span aria-hidden="true">{label}</span>
    </span>
  );
}

function seconds(value: number) {
  if (value < 60) return `${value} s`;
  if (value < 3600) return `${Math.round(value / 60)} min`;
  return `${Math.round(value / 3600)} h`;
}

/** What a view covers and how current it is: period, scope, last update and auto-refresh. */
export function FreshnessNote({
  scope, freshnessSeconds, updatedAt, refreshSeconds,
}: {
  scope: string;
  freshnessSeconds?: number | null;
  updatedAt: number;
  refreshSeconds?: number;
}) {
  const parts = [
    scope,
    freshnessSeconds ? `Evidence older than ${seconds(freshnessSeconds)} is stale` : "Freshness bound unavailable",
    updatedAt ? `Updated ${new Date(updatedAt).toLocaleTimeString()}` : "Not updated yet",
    refreshSeconds ? `Refreshes every ${seconds(refreshSeconds)}` : "Does not refresh automatically",
  ];
  return <p aria-label="View freshness" className="m-0 text-xs text-[var(--muted)]">{parts.join(" · ")}</p>;
}

/** Why a view or cell holds no trustworthy value. None of these is ever shown as zero. */
export type DataStateKind = "empty" | "incomplete" | "delayed" | "stale" | "unavailable" | "restricted";

const dataStates: Record<DataStateKind, { title: string; detail: string }> = {
  empty: { title: "No data", detail: "Nothing was recorded for this scope." },
  incomplete: { title: "Incomplete", detail: "Some sources did not answer; what is shown is partial." },
  delayed: { title: "Delayed", detail: "Telemetry is arriving later than expected." },
  stale: { title: "Stale", detail: "The newest evidence is older than the freshness bound." },
  unavailable: { title: "Unavailable", detail: "The source could not be read." },
  restricted: { title: "Restricted", detail: "You are not permitted to see this evidence." },
};

export function DataState({ kind, compact, children }: { kind: DataStateKind; compact?: boolean; children?: ReactNode }) {
  const { title, detail } = dataStates[kind];
  if (compact) {
    return <span data-data-state={kind} className="text-xs italic text-[var(--muted)]" title={detail}>{title}</span>;
  }
  return (
    <div role="status" data-data-state={kind} className="modern-panel grid gap-1 p-3 text-sm">
      <strong>{title}</strong>
      <span className="text-[var(--muted)]">{children ?? detail}</span>
    </div>
  );
}

/** Maps an evidence or issue status from the providers to a data state, or null when current. */
export function dataStateOf(status: string | null | undefined): DataStateKind | null {
  switch (status) {
    case "current": return null;
    case "missing": return "empty";
    case "stale": return "stale";
    case "restricted": return "restricted";
    case "insufficient": return "incomplete";
    case "delayed": return "delayed";
    default: return "unavailable";
  }
}

export interface Confidence {
  confidence: "full" | "degraded" | "unknown";
  affected: string[];
}

const functionLabels: Record<string, string> = {
  assess: "health assessment",
  detect: "alert detection",
  ingest: "telemetry ingest",
  report: "reporting",
};

/** Loss-of-confidence warning (OBS-29-03). A missing confidence is never treated as full. */
export function ConfidenceNotice({ confidence }: { confidence: Confidence | undefined }) {
  if (confidence?.confidence === "full") {
    return <p className="m-0 text-xs text-[var(--muted)]">Platform confidence: full</p>;
  }
  const affected = confidence?.affected.map((f) => functionLabels[f] ?? f) ?? [];
  const degraded = confidence?.confidence === "degraded";
  return (
    <div role="status" aria-label="Platform confidence" data-confidence={confidence?.confidence ?? "unknown"}
      className="modern-panel grid gap-1 border-l-4 border-[var(--warn)] p-3 text-sm">
      <strong>{degraded ? "Loss of confidence" : "Platform confidence unknown"}</strong>
      <span>
        {affected.length
          ? `${degraded ? "Behind or unavailable" : "Not yet confirmed"}: ${affected.join(", ")}. `
          : "The platform's own status could not be confirmed. "}
        States shown here may be out of date and are not fit for a decision.
      </span>
    </div>
  );
}

const unfitLabels: Record<string, string> = {
  not_live: "the service is not live",
  confidence_degraded: "platform confidence is degraded",
  confidence_unknown: "platform confidence is unknown",
};

/** Why a review is not fit for a decision, in words. */
export function unfitReason(reason: string) {
  if (unfitLabels[reason]) return unfitLabels[reason];
  if (reason.startsWith("evidence_")) return `evidence is ${reason.slice("evidence_".length).replace(/_/g, " ")}`;
  return reason.replace(/_/g, " ");
}
