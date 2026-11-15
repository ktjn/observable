import { useQueryClient } from "@tanstack/react-query";
import { useCallback } from "react";
import { switchTenant } from "../api/auth";
import { closeAllViewSubscriptions } from "../runtime/viewSubscriptionManager";
import { useTenantContext, type TenantInfo } from "./useTenantContext";

const IS_PLAYGROUND = import.meta.env.VITE_OBSERVABLE_RUNTIME === "playground";

/**
 * Switches the active tenant of the authenticated session.
 *
 * Ordering is the tenant-isolation guarantee (PUSH-014): every open read-only
 * view stream is closed *before* the session changes tenant, and the query cache
 * is cleared before the new tenant is adopted, so no previous-tenant snapshot or
 * in-flight pushed event can render as the new tenant's data. The local tenant
 * selection is only adopted once the server has authorized the switch.
 */
export function useTenantSwitch(): (tenant: TenantInfo) => Promise<void> {
  const queryClient = useQueryClient();
  const { setTenant } = useTenantContext();

  return useCallback(
    async (tenant: TenantInfo) => {
      closeAllViewSubscriptions();
      if (!IS_PLAYGROUND) {
        await switchTenant(tenant.id);
      }
      queryClient.clear();
      // Writes the tenant id to local storage, which is also how other tabs
      // sharing this session learn to drop their own streams and caches.
      setTenant(tenant);
      window.location.reload();
    },
    [queryClient, setTenant],
  );
}
