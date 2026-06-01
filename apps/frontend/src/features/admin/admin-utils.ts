import type { TimeFormat } from "../../lib/timeDisplay";
import { formatTimestamp } from "../../utils/formatTimestamp";

export type BadgeTone = "good" | "warn" | "bad" | "info" | "neutral";

export function formatInterval(fromMs: number, toMs: number, format: TimeFormat): string {
  return `${formatTimestamp(fromMs * 1_000_000, format)} to ${formatTimestamp(toMs * 1_000_000, format)}`;
}

export function countTone(value: number): "good" | "warn" | "bad" | "info" {
  if (value === 0) return "good";
  if (value > 1000) return "bad";
  if (value > 100) return "warn";
  return "info";
}

export function roleLabel(role?: string): string {
  switch (role) {
    case "tenant_admin":
      return "Tenant admin";
    case "project_admin":
      return "Project admin";
    case "member":
      return "Member";
    case "viewer":
      return "Viewer";
    default:
      return "Unassigned";
  }
}

export function roleTone(role?: string): BadgeTone {
  switch (role) {
    case "tenant_admin":
      return "good";
    case "project_admin":
      return "info";
    case "member":
      return "neutral";
    case "viewer":
      return "warn";
    default:
      return "neutral";
  }
}
