import { createContext, useContext, useMemo } from "react";
import type { ReactNode } from "react";
import { httpRuntime } from "../runtime/httpRuntime";
import { playgroundRuntime } from "../runtime/playgroundRuntime";
import type { RuntimeApi } from "../runtime/types";

// ── Context ───────────────────────────────────────────────────────────────────

const RuntimeContext = createContext<RuntimeApi | undefined>(undefined);

// ── Provider ──────────────────────────────────────────────────────────────────

/**
 * Selects the production HTTP runtime or the browser-local playground runtime
 * at build time via `VITE_OBSERVABLE_RUNTIME`. Feature components must depend
 * only on `useRuntime()`, never on the transport behind it.
 */
export function RuntimeProvider({ children }: { children: ReactNode }) {
  const runtime = useMemo<RuntimeApi>(
    () => (import.meta.env.VITE_OBSERVABLE_RUNTIME === "playground" ? playgroundRuntime : httpRuntime),
    []
  );

  return <RuntimeContext.Provider value={runtime}>{children}</RuntimeContext.Provider>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useRuntime(): RuntimeApi {
  const value = useContext(RuntimeContext);
  if (!value) {
    throw new Error("useRuntime must be used inside RuntimeProvider");
  }
  return value;
}
