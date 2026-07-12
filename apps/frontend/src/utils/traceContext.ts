import type { TraceResponse } from "../api/traces";
import type { TimeFormat } from "../lib/timeDisplay";
import { formatTimestamp } from "./formatTimestamp";
import { formatContextValue } from "./logFormatting";
import { formatStatusLabel } from "./traceStatus";

export const FIXED_TRACE_KEYS = [
  "start_time",
  "trace_id",
  "service.name",
  "operation",
  "duration",
  "status",
] as const;

export const DEFAULT_TRACE_COLUMNS = FIXED_TRACE_KEYS;

export function getTraceFieldValue(trace: TraceResponse, key: string, format: TimeFormat): string {
  const root = trace.spans[0];
  if (key === "trace_id") return trace.trace_id;
  if (!root) return "";

  switch (key) {
    case "start_time":
      return formatTimestamp(root.start_time_unix_nano, format);
    case "service.name":
      return root.service_name;
    case "operation":
      return root.operation_name;
    case "duration":
      return `${(root.duration_ns / 1_000_000).toFixed(2)}ms`;
    case "status":
      return formatStatusLabel(root.status_code);
    default:
      if (key.startsWith("span.")) return formatContextValue(root.attributes?.[key.slice(5)]);
      if (key.startsWith("resource."))
        return formatContextValue(root.resource_attributes?.[key.slice("resource.".length)]);
      return "";
  }
}

export function traceContextEntries(trace: TraceResponse, format: TimeFormat): [string, string][] {
  const root = trace.spans[0];
  if (!root) return [];
  const entries: [string, string][] = FIXED_TRACE_KEYS.map((key) => [
    key,
    getTraceFieldValue(trace, key, format),
  ]);
  for (const [key, value] of Object.entries(root.attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`span.${key}`, formatContextValue(value)]);
  for (const [key, value] of Object.entries(root.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`resource.${key}`, formatContextValue(value)]);
  return entries;
}
