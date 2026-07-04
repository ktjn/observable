/**
 * Shared utilities for OTel severity classification and log body rendering.
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

const TONE_TEXT_CLASS: Record<ReturnType<typeof otelSeverity>["tone"], string> = {
  good: "text-[var(--good)]",
  warn: "text-[var(--warn)]",
  bad: "text-[var(--bad)]",
  info: "text-[var(--brand)]",
  neutral: "text-[var(--muted)]",
};

/**
 * Tailwind text-colour class for a log level, wherever it's shown as plain
 * colour-coded text rather than a Badge (a bordered chip reads as a button
 * even inline, so log level is deliberately plain text everywhere).
 */
export function severityTextClass(severity: number): string {
  return TONE_TEXT_CLASS[otelSeverity(severity).tone];
}

export function formatContextValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (value === null || value === undefined) return "";
  return JSON.stringify(value);
}

export function formatLogMessage(body: unknown): string {
  if (typeof body === "string") {
    try {
      const parsed: unknown = JSON.parse(body);
      if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
        const record = parsed as Record<string, unknown>;
        const message = record.message ?? record.msg ?? record.body;
        if (typeof message === "string") return message;
        return Object.entries(record)
          .map(([key, value]) => `${key}=${formatContextValue(value)}`)
          .join(" ");
      }
    } catch {
      // Not JSON — return raw string
    }
    return body;
  }
  if (typeof body === "number" || typeof body === "boolean") return String(body);
  if (!body || typeof body !== "object") return String(body ?? "");
  if (Array.isArray(body)) return JSON.stringify(body);

  const record = body as Record<string, unknown>;
  const message = record.message ?? record.msg ?? record.body;
  if (typeof message === "string") return message;

  return Object.entries(record)
    .map(([key, value]) => `${key}=${formatContextValue(value)}`)
    .join(" ");
}
