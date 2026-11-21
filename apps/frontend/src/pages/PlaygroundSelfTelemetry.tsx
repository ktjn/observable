import { useEffect, useState } from "react";
import type { SystemLogRow } from "../playground/selfTelemetry";

/**
 * Dev-only harness for the playground engine's self-instrumentation
 * (ADR-047 §4): queries its own startup/live-tick/query-cost logs, written
 * under the system tenant, through the same `DuckDbLogQueryApi` path any
 * other log query uses. Not linked from nav, not exposed through the
 * tenant switcher — ADR-047 §5's "one implicit tenant" stays true for
 * user-visible data.
 *
 * Registered only in playground builds (see `router.ts`).
 */

declare global {
  interface Window {
    __observableSelfTelemetry?: SystemLogRow[];
  }
}

export default function PlaygroundSelfTelemetry() {
  const [logs, setLogs] = useState<SystemLogRow[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        // Dynamically imported (not a static import): engineClient statically
        // references a Worker URL, which Vite's dep scanner eagerly resolves
        // at import time — see the identical reasoning in AppShell.tsx.
        const { queryEngineSelfTelemetry } = await import("../playground/engineClient");
        const { logs: result } = await queryEngineSelfTelemetry(200);
        if (cancelled) return;
        window.__observableSelfTelemetry = result;
        setLogs(result);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div style={{ padding: "2rem", fontFamily: "monospace" }}>
      <h1>Playground engine self-telemetry</h1>
      {error && <p data-testid="self-telemetry-error">{error}</p>}
      <p data-testid="self-telemetry-count">{logs?.length ?? 0}</p>
      <ul data-testid="self-telemetry-rows">
        {(logs ?? []).map((log) => (
          <li key={log.log_id} data-testid="self-telemetry-row">
            <span data-testid="self-telemetry-severity">{log.severity_text}</span>{" "}
            <span data-testid="self-telemetry-body">{log.body}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
