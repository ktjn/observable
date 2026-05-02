import type { TimeFormat } from "../lib/timeDisplay";

export function formatBucketLabel(ms: number, format: TimeFormat): string {
  const utc =
    format === "iso-utc-ms" ||
    format === "iso-utc-ns" ||
    format === "unix-ms" ||
    format === "unix-ns";
  return utc ? new Date(ms).toISOString() : new Date(ms).toLocaleTimeString();
}
