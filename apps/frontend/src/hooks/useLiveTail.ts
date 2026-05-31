import { useEffect, useRef, useState } from "react";
import { tailLogs } from "../api/logs";
import type { LogRecord } from "../api/logs";
import { LIVE_VIEW_REFRESH_INTERVAL_MS } from "./useLiveRefresh";

const MAX_LIVE_ROWS = 500;

/**
 * Concatenate `prev` and `next`, then return the last `maxRows` entries.
 * Exported for unit testing.
 */
export function appendAndTrim<T>(prev: T[], next: T[], maxRows: number): T[] {
  const combined = [...prev, ...next];
  return combined.length > maxRows
    ? combined.slice(combined.length - maxRows)
    : combined;
}

export interface UseLiveTailOptions {
  tenantId: string;
  service?: string;
  severityMin?: number;
  enabled: boolean;
}

export interface UseLiveTailResult {
  logs: LogRecord[];
  error: Error | null;
}

export function useLiveTail(opts: UseLiveTailOptions): UseLiveTailResult {
  const { tenantId, service, severityMin, enabled } = opts;
  const [logs, setLogs] = useState<LogRecord[]>([]);
  const [error, setError] = useState<Error | null>(null);
  const cursorRef = useRef<string>("");

  useEffect(() => {
    if (!enabled) {
      setLogs([]);
      setError(null);
      return;
    }

    cursorRef.current = String(Date.now() * 1_000_000);
    let aborted = false;

    const tick = async () => {
      try {
        const res = await tailLogs(tenantId, {
          service,
          severity: severityMin,
          since_unix_nano: cursorRef.current,
          limit: 100,
        });
        if (aborted) return;
        if (res.logs.length > 0) {
          setLogs((prev) => appendAndTrim(prev, res.logs, MAX_LIVE_ROWS));
          const newest = res.logs.reduce(
            (max, l) =>
              BigInt(l.timestamp_unix_nano) > BigInt(max)
                ? l.timestamp_unix_nano
                : max,
            cursorRef.current
          );
          cursorRef.current = newest;
        }
        setError(null);
      } catch (e) {
        if (aborted) return;
        setError(e instanceof Error ? e : new Error(String(e)));
      }
    };

    tick();
    const id = setInterval(tick, LIVE_VIEW_REFRESH_INTERVAL_MS);
    return () => {
      aborted = true;
      clearInterval(id);
    };
  }, [enabled, tenantId, service, severityMin]);

  return { logs, error };
}
