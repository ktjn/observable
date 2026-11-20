import { useCallback, useEffect, useRef, useState } from "react";
import type { LogRecord } from "../api/logs";
import { LIVE_VIEW_REFRESH_INTERVAL_MS } from "./useLiveRefresh";
import { useRuntime } from "./useRuntime";

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
  pollIntervalMs?: number | false;
  /**
   * True while a push source (e.g. an SSE delta) is driving `refresh()`
   * directly, so the interval fallback below is suppressed. Toggling this
   * must never reset the cursor or accumulated rows — only the interval
   * scheduling effect reacts to it.
   */
  pushActive?: boolean;
}

export interface UseLiveTailResult {
  logs: LogRecord[];
  error: Error | null;
  refresh: () => Promise<void>;
}

export function useLiveTail(opts: UseLiveTailOptions): UseLiveTailResult {
  const {
    tenantId,
    service,
    severityMin,
    enabled,
    pollIntervalMs = LIVE_VIEW_REFRESH_INTERVAL_MS,
    pushActive = false,
  } = opts;
  const runtime = useRuntime();
  const [logs, setLogs] = useState<LogRecord[]>([]);
  const [error, setError] = useState<Error | null>(null);
  const cursorRef = useRef<string>("");
  const tickRef = useRef<() => Promise<void>>(async () => {});

  // Owns the cursor, abort flag and tick() function. Must not restart on
  // pollIntervalMs/pushActive changes alone: that would reset cursorRef to
  // "now" and drop whatever rows landed between the old cursor and the
  // reset, which is exactly the gap a push-driven tail must not have on a
  // reconnect blip.
  useEffect(() => {
    if (!enabled) {
      setLogs([]);
      setError(null);
      tickRef.current = async () => {};
      return;
    }

    cursorRef.current = String(Date.now() * 1_000_000);
    let aborted = false;

    const tick = async () => {
      try {
        const res = await runtime.logs.tail(tenantId, {
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
                ? String(l.timestamp_unix_nano)
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

    tickRef.current = tick;
    void tick();
    return () => {
      aborted = true;
      tickRef.current = async () => {};
    };
  }, [enabled, tenantId, service, severityMin]);

  // Owns only the interval timer. A push source (SSE delta calling
  // refresh()) suppresses this; it is the fallback for when nothing else is
  // driving tick(). Restarting this effect is cheap: it never touches the
  // cursor or accumulated rows.
  useEffect(() => {
    if (!enabled || pushActive) return;
    if (pollIntervalMs === false || pollIntervalMs <= 0) return;
    const id = setInterval(() => void tickRef.current(), pollIntervalMs);
    return () => clearInterval(id);
  }, [enabled, pushActive, pollIntervalMs]);

  const refresh = useCallback(() => tickRef.current(), []);

  return { logs, error, refresh };
}
