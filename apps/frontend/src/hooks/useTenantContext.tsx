import { createContext, useContext, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { LOCAL_DEV_TENANT, LOCAL_DEV_TENANT_ID } from "../api/setup";

// ── Types ─────────────────────────────────────────────────────────────────────

export interface TenantInfo {
  id: string;
  name: string;
}

interface TenantContextValue {
  /** The currently selected tenant id (UUID string). */
  tenantId: string;
  /** The currently selected tenant display name. */
  tenantName: string;
  /**
   * The currently selected environment, or null when "all environments" is
   * selected (i.e. no environment filter applied).
   */
  environment: string | null;
  /** Switch to a different tenant. Resets the environment selection to null. */
  setTenant: (tenant: TenantInfo) => void;
  /** Set the environment filter. Pass null to select "all environments". */
  setEnvironment: (env: string | null) => void;
}

// ── Context ───────────────────────────────────────────────────────────────────

const TenantContext = createContext<TenantContextValue | undefined>(undefined);

// ── Provider ──────────────────────────────────────────────────────────────────

/**
 * Provides the global tenant + environment scope to the entire application.
 *
 * Default: the self-ingestion (system/platform) tenant with no environment
 * filter ("all environments").  This default mirrors the seeded dev tenant
 * and is the only meaningful selection before authentication is in place.
 *
 * When authentication arrives the list of available tenants will be filtered
 * by the principal's permissions; the context itself needs no structural change.
 */
export function TenantContextProvider({ children }: { children: ReactNode }) {
  const [tenantId, setTenantId] = useState(LOCAL_DEV_TENANT_ID);
  const [tenantName, setTenantName] = useState(LOCAL_DEV_TENANT);
  const [environment, setEnvironmentState] = useState<string | null>(null);

  const value = useMemo<TenantContextValue>(
    () => ({
      tenantId,
      tenantName,
      environment,
      setTenant({ id, name }) {
        setTenantId(id);
        setTenantName(name);
        setEnvironmentState(null); // reset on tenant change
      },
      setEnvironment(env) {
        setEnvironmentState(env);
      },
    }),
    [tenantId, tenantName, environment]
  );

  return <TenantContext.Provider value={value}>{children}</TenantContext.Provider>;
}

// ── Hook ──────────────────────────────────────────────────────────────────────

export function useTenantContext(): TenantContextValue {
  const value = useContext(TenantContext);
  if (!value) {
    throw new Error("useTenantContext must be used inside TenantContextProvider");
  }
  return value;
}
