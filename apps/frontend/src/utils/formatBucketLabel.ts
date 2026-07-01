import type { TimeFormat } from "../lib/timeDisplay";

export function formatBucketLabel(ms: number, format: TimeFormat): string {
  const utc =
    format === "iso-utc-ms" ||
    format === "iso-utc-ns" ||
    format === "unix-ms" ||
    format === "unix-ns";
  const d = new Date(ms);
  if (utc) {
    const hh = d.getUTCHours().toString().padStart(2, "0");
    const mm = d.getUTCMinutes().toString().padStart(2, "0");
    return `${hh}:${mm} UTC`;
  }
  return d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}
