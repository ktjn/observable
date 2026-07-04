import type { Span } from "../api/traces";

/**
 * Instrumentation sets ERROR explicitly but rarely sets OK, so most spans
 * carry the raw OTel "UNSET" status. Render UNSET the same as OK since
 * neither indicates a problem.
 */
export function formatStatusLabel(statusCode: Span["status_code"]): "OK" | "ERROR" {
  return statusCode === "ERROR" ? "ERROR" : "OK";
}
