import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import { createViewSubscriptionManager } from "../runtime/viewSubscriptionManager";
import type { ViewSubscriptionState } from "../runtime/viewSubscriptionManager";
import { encodeViewsParam } from "../runtime/viewSubscriptionProtocol";
import type {
  ViewSubscriptionDefinition,
  ViewSubscriptionTransport,
} from "../runtime/viewSubscriptionProtocol";
import { useRuntime } from "./useRuntime";
import { LS_TENANT_ID } from "./useTenantContext";

const noopTransport: ViewSubscriptionTransport = {
  open: () => ({ close() {} }),
};

export interface UseReadonlyViewSubscriptionOptions {
  /** The tenant the caller believes is active; the stream only opens if the
   *  authoritative session agrees. */
  tenantId: string;
  /** The read-only views this view wants pushed. Pinned ranges are not
   *  subscribed and report `paused`. */
  views: readonly ViewSubscriptionDefinition[];
  enabled?: boolean;
}

/**
 * Subscribes a read-only view to server-pushed updates, applying snapshots,
 * deltas and replacements straight to the TanStack Query keys the view already
 * renders from — no fixed-interval polling and no refetch after a data event.
 *
 * The hook returns status only, never view data: data lives in the query cache,
 * so a reconnect, a reset or a view-set change updates a single status field and
 * cannot remount the consumer or discard its selection, filters, scroll position
 * or unsaved form state (PUSH-005).
 */
export function useReadonlyViewSubscription(
  options: UseReadonlyViewSubscriptionOptions,
): ViewSubscriptionState {
  const { tenantId, views, enabled = true } = options;
  const runtime = useRuntime();
  const queryClient = useQueryClient();

  const manager = useMemo(
    () =>
      createViewSubscriptionManager({
        queryClient,
        transport: runtime.viewSubscriptions?.transport ?? noopTransport,
        confirmTenant: () =>
          runtime.viewSubscriptions
            ? runtime.viewSubscriptions.confirmTenant()
            : Promise.resolve(null),
      }),
    [queryClient, runtime],
  );

  const state = useSyncExternalStore(manager.subscribe, manager.getState, manager.getState);

  // The wire identity of the view set: a re-render that does not change it must
  // not disturb the stream.
  const viewsKey = encodeViewsParam(views);
  const viewsRef = useRef(views);
  viewsRef.current = views;
  const active = enabled && Boolean(runtime.viewSubscriptions);

  useEffect(() => {
    manager.update({ tenantId, views: viewsRef.current, enabled: active });
  }, [manager, tenantId, viewsKey, active]);

  useEffect(() => () => manager.dispose(), [manager]);

  // A tenant switch in another tab shares this session cookie, so the stream
  // here is obsolete the moment that tab switches: stop it and drop its results
  // rather than waiting for the server to terminate it (PUSH-014).
  useEffect(() => {
    function onStorage(event: StorageEvent) {
      if (event.key !== LS_TENANT_ID) return;
      if (event.newValue === null || event.newValue === tenantId) return;
      manager.closeAndIsolate();
    }
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [manager, tenantId]);

  return state;
}
