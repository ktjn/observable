import type { LogRecord } from "../api/logs";
import type { TimeFormat } from "../lib/timeDisplay";
import { formatTimestamp } from "./formatTimestamp";
import { formatContextValue, formatLogMessage } from "./logFormatting";

/** Fixed (non-attribute) log context keys, in display order. */
export const FIXED_LOG_KEYS = [
  "time",
  "service.name",
  "severity_number",
  "message",
  "observed_time",
  "environment",
  "host_id",
  "trace_id",
  "span_id",
  "fingerprint",
] as const;

/** Default table columns, using the same canonical keys as the context panel and saved views. */
export const DEFAULT_LOG_COLUMNS = ["time", "severity_number", "service.name", "message"] as const;

const FIXED_LOG_KEY_SET = new Set<string>(FIXED_LOG_KEYS);

export function normalizeLogColumnKeys(keys: readonly string[]): string[] {
  const normalized: string[] = [];
  const seen = new Set<string>();
  for (const key of keys) {
    const canonical =
      key === "level"
        ? "severity_number"
        : key === "service"
          ? "service.name"
          : FIXED_LOG_KEY_SET.has(key) || key.startsWith("log.") || key.startsWith("resource.")
            ? key
            : `resource.${key}`;
    if (!seen.has(canonical)) {
      seen.add(canonical);
      normalized.push(canonical);
    }
  }
  return normalized;
}

/**
 * Resolves the formatted display value for a log context key — either a
 * fixed field, a `log.<attr>` attribute, or a `resource.<attr>` resource attribute key.
 * Shared between the log context sidebar (per-selected-log) and the results
 * table (per-row, for promoted columns) so both stay in sync.
 */
export function getLogFieldValue(log: LogRecord, key: string, format: TimeFormat): string {
  switch (key) {
    case "time":
      return formatTimestamp(log.timestamp_unix_nano, format);
    case "service.name":
      return log.service_name;
    case "severity_number":
      return String(log.severity_number);
    case "message":
      return formatLogMessage(log.body);
    case "observed_time":
      return log.observed_timestamp_unix_nano
        ? formatTimestamp(log.observed_timestamp_unix_nano, format)
        : "";
    case "environment":
      return log.environment ?? "";
    case "host_id":
      return log.host_id ?? "";
    case "trace_id":
      return log.trace_id ?? "";
    case "span_id":
      return log.span_id ?? "";
    case "fingerprint":
      return log.fingerprint !== null && log.fingerprint !== undefined
        ? String(log.fingerprint)
        : "";
    default:
      if (key.startsWith("log.")) return formatContextValue(log.attributes?.[key.slice(4)]);
      if (key.startsWith("resource."))
        return formatContextValue(log.resource_attributes?.[key.slice("resource.".length)]);
      return "";
  }
}

export function logContextEntries(log: LogRecord, format: TimeFormat): [string, string][] {
  const entries: [string, string][] = [
    ["time", getLogFieldValue(log, "time", format)],
    ["service.name", getLogFieldValue(log, "service.name", format)],
    ["severity_number", getLogFieldValue(log, "severity_number", format)],
    ["message", getLogFieldValue(log, "message", format)],
  ];
  if (log.observed_timestamp_unix_nano)
    entries.push(["observed_time", getLogFieldValue(log, "observed_time", format)]);
  if (log.environment) entries.push(["environment", log.environment]);
  if (log.host_id) entries.push(["host_id", log.host_id]);
  if (log.trace_id) entries.push(["trace_id", log.trace_id]);
  if (log.span_id) entries.push(["span_id", log.span_id]);
  if (log.fingerprint !== null && log.fingerprint !== undefined)
    entries.push(["fingerprint", String(log.fingerprint)]);
  for (const [k, v] of Object.entries(log.attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`log.${k}`, formatContextValue(v)]);
  for (const [k, v] of Object.entries(log.resource_attributes ?? {}).sort(([a], [b]) => a.localeCompare(b)))
    entries.push([`resource.${k}`, formatContextValue(v)]);
  return entries;
}
