import { useEffect, useState } from "react";
import { runPlaygroundSpike } from "../playground/workerClient";

type SpikeState =
  | { status: "loading" }
  | { status: "ready"; ping: string; rowCount: number }
  | { status: "error"; message: string };

/**
 * Phase 0 viability spike — not a real feature page. Proves that under this
 * build's base path, a Web Worker can load the Rust/wasm-bindgen output and
 * round-trip a DuckDB-WASM query with no HTTP calls to an Observable backend.
 * See docs/superpowers/plans/2026-08-21-github-pages-wasm-playground.md.
 */
export default function PlaygroundSpike() {
  const [state, setState] = useState<SpikeState>({ status: "loading" });

  useEffect(() => {
    let cancelled = false;
    runPlaygroundSpike(42)
      .then(({ ping, rowCount }) => {
        if (!cancelled) setState({ status: "ready", ping, rowCount });
      })
      .catch((err: unknown) => {
        if (!cancelled) {
          setState({ status: "error", message: err instanceof Error ? err.message : String(err) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div style={{ padding: "2rem", fontFamily: "monospace" }}>
      <h1>Playground spike</h1>
      {state.status === "loading" && <p data-testid="spike-loading">Loading WASM + DuckDB-WASM…</p>}
      {state.status === "ready" && (
        <p data-testid="spike-ready">
          ok: ping=&quot;{state.ping}&quot; rowCount={state.rowCount}
        </p>
      )}
      {state.status === "error" && <p data-testid="spike-error">error: {state.message}</p>}
    </div>
  );
}
