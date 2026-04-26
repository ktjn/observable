/**
 * Formats a unix nanosecond timestamp string as ISO 8601 with full 9-digit
 * fractional seconds precision (JavaScript Date only carries milliseconds, so
 * the remaining 6 sub-millisecond digits are taken directly from the raw nanos
 * string and appended).
 *
 * @param nanos - Unix timestamp as a nanosecond string (e.g. "1745659909123456789")
 * @param utc   - When true, format in UTC (e.g. "2026-04-26T09:35:09.123456789Z");
 *               when false, format in the browser's local timezone
 *               (e.g. "2026-04-26T11:35:09.123456789")
 */
export function formatTimestamp(nanos: string, utc: boolean): string {
  const ms = Math.floor(Number(nanos) / 1_000_000);
  const date = new Date(ms);
  // Last 6 digits of the nanos string carry microseconds + nanoseconds.
  const subMs = nanos.slice(-6).padStart(6, "0");

  if (utc) {
    // toISOString() → "YYYY-MM-DDTHH:mm:ss.mmmZ"; insert the 6 sub-ms digits before the trailing Z.
    return date.toISOString().replace(/Z$/, `${subMs}Z`);
  }

  const pad = (n: number, len = 2) => String(n).padStart(len, "0");
  // Use local Date getters so the displayed time matches the browser's timezone.
  const yyyy = date.getFullYear();
  const mo = pad(date.getMonth() + 1);
  const dd = pad(date.getDate());
  const hh = pad(date.getHours());
  const min = pad(date.getMinutes());
  const sec = pad(date.getSeconds());
  const msStr = pad(date.getMilliseconds(), 3);
  return `${yyyy}-${mo}-${dd}T${hh}:${min}:${sec}.${msStr}${subMs}`;
}
