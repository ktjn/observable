import type { TimeFormat } from "../lib/timeDisplay";

/** Convert an ISO-8601 timestamp string to nanoseconds (as a number). */
export function isoToNs(iso: string): number {
  return Date.parse(iso) * 1_000_000;
}

export function formatTimestamp(nanos: string | number, format: TimeFormat): string {
  const nanosStr = String(nanos);
  const ms = Math.floor(Number(nanosStr) / 1_000_000);
  const date = new Date(ms);
  const pad = (n: number, len = 2) => String(n).padStart(len, "0");

  // Sub-millisecond nanoseconds (last 6 digits of a 19-digit nano string)
  const subMs = nanosStr.length >= 16 ? nanosStr.slice(-6).padStart(6, "0") : "000000";

  switch (format) {
    case "iso-utc-ms": {
      const yyyy = date.getUTCFullYear();
      const mo = pad(date.getUTCMonth() + 1);
      const dd = pad(date.getUTCDate());
      const hh = pad(date.getUTCHours());
      const min = pad(date.getUTCMinutes());
      const sec = pad(date.getUTCSeconds());
      const msStr = pad(date.getUTCMilliseconds(), 3);
      return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}Z`;
    }
    case "iso-utc-ns": {
      const yyyy = date.getUTCFullYear();
      const mo = pad(date.getUTCMonth() + 1);
      const dd = pad(date.getUTCDate());
      const hh = pad(date.getUTCHours());
      const min = pad(date.getUTCMinutes());
      const sec = pad(date.getUTCSeconds());
      const msStr = pad(date.getUTCMilliseconds(), 3);
      return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}${subMs}Z`;
    }
    case "iso-local-ns": {
      const yyyy = date.getFullYear();
      const mo = pad(date.getMonth() + 1);
      const dd = pad(date.getDate());
      const hh = pad(date.getHours());
      const min = pad(date.getMinutes());
      const sec = pad(date.getSeconds());
      const msStr = pad(date.getMilliseconds(), 3);
      return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}${subMs}`;
    }
    case "unix-ms":
      return String(ms);
    case "unix-ns":
      return nanosStr;
    case "iso-local-ms":
    default: {
      const yyyy = date.getFullYear();
      const mo = pad(date.getMonth() + 1);
      const dd = pad(date.getDate());
      const hh = pad(date.getHours());
      const min = pad(date.getMinutes());
      const sec = pad(date.getSeconds());
      const msStr = pad(date.getMilliseconds(), 3);
      return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}`;
    }
  }
}

