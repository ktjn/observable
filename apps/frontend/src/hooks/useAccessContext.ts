import { useEffect, useState } from "react";
import { useAuth } from "./useAuth";
import { useTenantContext } from "./useTenantContext";

const READ_ONLY_ROLES = new Set(["viewer", "business_viewer", "read_only", "readonly"]);

/**
 * Resolves the caller's active organizational role, incident-mutation capability,
 * and time-bound elevated-access state (OBS-25-02, OBS-25-03, UX-10).
 */
export function useAccessContext() {
  const { data: user } = useAuth();
  const { tenantId, tenantName } = useTenantContext();
  const [nowMs, setNowMs] = useState(() => Date.now());

  const membership = user?.tenants?.find((t) => t.tenant_id === tenantId) ?? user?.tenants?.[0];
  const role = membership?.role ?? (user ? "member" : "viewer");
  const canMutateIncidents = Boolean(user) && !READ_ONLY_ROLES.has(role.toLowerCase());

  const rawElevation = user?.elevated_access ?? null;
  const expiresAtMs = rawElevation?.expires_at ? Date.parse(rawElevation.expires_at) : Number.NaN;
  const hasExpiry = Number.isFinite(expiresAtMs);

  useEffect(() => {
    if (!rawElevation?.active || !hasExpiry) return;
    const remainingMs = expiresAtMs - Date.now();
    if (remainingMs <= 0) {
      setNowMs(Date.now());
      return;
    }
    const timer = window.setTimeout(() => setNowMs(Date.now()), remainingMs + 25);
    return () => window.clearTimeout(timer);
  }, [rawElevation?.active, hasExpiry, expiresAtMs]);

  const isElevated = Boolean(
    rawElevation?.active && (!hasExpiry || expiresAtMs > nowMs),
  );

  return {
    user: user ?? null,
    role,
    tenantId,
    tenantName,
    canMutateIncidents,
    isElevated,
    elevation: isElevated ? rawElevation : null,
    impersonating: user?.impersonating ?? null,
  };
}
