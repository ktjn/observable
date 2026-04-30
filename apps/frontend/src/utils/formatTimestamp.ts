export function formatTimestamp(nanos: string, utc: boolean): string {
  const ms = Math.floor(Number(String(nanos)) / 1_000_000);
  const date = new Date(ms);
  const pad = (n: number, len = 2) => String(n).padStart(len, "0");

  if (utc) {
    const yyyy = date.getUTCFullYear();
    const mo = pad(date.getUTCMonth() + 1);
    const dd = pad(date.getUTCDate());
    const hh = pad(date.getUTCHours());
    const min = pad(date.getUTCMinutes());
    const sec = pad(date.getUTCSeconds());
    const msStr = pad(date.getUTCMilliseconds(), 3);
    return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}Z`;
  }

  const yyyy = date.getFullYear();
  const mo = pad(date.getMonth() + 1);
  const dd = pad(date.getDate());
  const hh = pad(date.getHours());
  const min = pad(date.getMinutes());
  const sec = pad(date.getSeconds());
  const msStr = pad(date.getMilliseconds(), 3);
  return `${yyyy}-${mo}-${dd} ${hh}:${min}:${sec}.${msStr}`;
}
