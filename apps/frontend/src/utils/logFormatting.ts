/**
 * Shared log formatting utilities for OTel severity levels, log message
 * formatting, and severity colour helpers.
 *
 * These functions are extracted from LogSearch.tsx so they can be reused by
 * other components (e.g. LogView, histogram overlays) without importing the
 * full page module.
 */

export type OTelLevel = "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR" | "FATAL";

export function otelSeverity(severity: number): { label: OTelLevel; tone: "good" | "warn" | "bad" | "info" | "neutral" } {
  if (severity >= 21) return { label: "FATAL", tone: "bad" };
  if (severity >= 17) return { label: "ERROR", tone: "bad" };
  if (severity >= 13) return { label: "WARN", tone: "warn" };
  if (severity >= 9) return { label: "INFO", tone: "good" };
  if (severity >= 5) return { label: "DEBUG", tone: "info" };
  return { label: "TRACE", tone: "neutral" };
}

export function formatContextValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value === null || value === undefined) return "";
  return JSON.stringify(value);
}

export function formatLogMessage(body: unknown): string {
  if (typeof body === "string") return body;
  if (typeof body === "number" || typeof body === "boolean") return String(body);
  if (!body || typeof body !== "object" || Array.isArray(body)) return String(body ?? "");

  const record = body as Record<string, unknown>;
  const message = record.message ?? record.msg ?? record.body;
  if (typeof message === "string") return message;

  return Object.entries(record)
    .map(([key, value]) => `${key}=${formatContextValue(value)}`)
    .join(" ");
}

/**
 * Returns the CSS variable string for the colour that corresponds to the given
 * OTel severity number.  Consolidates three identical duplicate implementations
 * that previously existed across the codebase.
 */
export function getSeverityColor(severity: number): string {
  if (severity >= 13) return "var(--bad)";
  if (severity >= 9) return "var(--warn)";
  if (severity >= 5) return "var(--brand)";
  return "var(--muted)";
}
