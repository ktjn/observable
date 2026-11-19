import type { QueryClient } from "@tanstack/react-query";
import {
  applyKeyedChanges,
  compareRevisions,
  encodeViewsParam,
  isPinnedTimeRange,
} from "./viewSubscriptionProtocol";
import type {
  ViewAckItem,
  ViewStreamErrorKind,
  ViewStreamHandle,
  ViewSubscriptionDefinition,
  ViewSubscriptionEvent,
  ViewSubscriptionTransport,
} from "./viewSubscriptionProtocol";

/**
 * The single read-only view stream of a browser tab: it owns the connection
 * lifecycle, applies authorized data events straight to the TanStack Query cache,
 * and exposes the status a view renders (PUSH-004/005/009/014/015/019).
 *
 * The manager never touches the network itself. It drives a
 * `ViewSubscriptionTransport`, so the real `EventSource` and the playground fake
 * are the same mechanism from a component's point of view.
 */

export type ViewSubscriptionStatus =
  /** Nothing subscribed. */
  | "idle"
  /** Only pinned historical views; event delivery must not change them. */
  | "paused"
  /** Registration in flight. */
  | "connecting"
  /** Connected; data events are being applied. */
  | "live"
  /** Dropped; retrying with backoff while cached data stays visible. */
  | "reconnecting"
  /** Repeated reconnect failures; bounded query polling is covering the gap. */
  | "stale"
  /** The authoritative session does not authorize this view set. */
  | "unauthorized";

export interface ViewSubscriptionState {
  status: ViewSubscriptionStatus;
  /** When a data event was last applied, for a "last updated" indicator. */
  lastUpdatedAt: number | null;
  lastResetReason: string | null;
}

export interface ViewSubscriptionManagerConfig {
  queryClient: QueryClient;
  transport: ViewSubscriptionTransport;
  /**
   * Resolves the authoritative active session tenant (`/v1/auth/me`). The manager
   * refuses to open a stream until this agrees with the tenant it was asked to
   * subscribe for, so a tenant switch cannot be raced (PUSH-014).
   */
  confirmTenant: () => Promise<string | null>;
  reconnectDelaysMs?: readonly number[];
  staleAfterAttempts?: number;
  fallbackPollIntervalMs?: number;
  /**
   * Coalescing window (ms) for bounded batched applied-revision ACKs (PUSH-023).
   * Defaults to 250 ms; set `<= 0` to disable client ACK reporting.
   */
  ackBatchMs?: number;
  /**
   * Optional override for sending batched applied-revision ACKs. Defaults to
   * `transport.ack` when present.
   */
  ack?: (streamId: string, acks: readonly ViewAckItem[]) => Promise<void>;
}

export interface ViewSubscriptionUpdate {
  tenantId: string;
  views: readonly ViewSubscriptionDefinition[];
  enabled?: boolean;
}

export interface ViewSubscriptionManager {
  getState(): ViewSubscriptionState;
  subscribe(listener: () => void): () => void;
  /**
   * Declares the view set this tab wants. Reopens the stream only when the
   * subscribed definitions actually changed, so a re-render with an equal view
   * set never interrupts the stream or the component tree below it.
   */
  update(update: ViewSubscriptionUpdate): void;
  /** Stops streaming and leaves cached view data in place. */
  close(): void;
  /**
   * Stops streaming and drops the subscribed views' cached data. Used on tenant
   * switch and logout so no previous-tenant result can render.
   */
  closeAndIsolate(): void;
  /** Closes and unregisters from the process-wide registry. */
  dispose(): void;
}

const DEFAULT_RECONNECT_DELAYS_MS = [1_000, 2_000, 5_000, 10_000, 30_000] as const;
const DEFAULT_STALE_AFTER_ATTEMPTS = 3;
const DEFAULT_FALLBACK_POLL_INTERVAL_MS = 15_000;
const DEFAULT_ACK_BATCH_MS = 250;

interface AppliedRevision {
  queryRevision: string;
  revision: string | null;
}

const registry = new Set<ViewSubscriptionManager>();

/**
 * Closes every open view stream in this tab. Called on logout and tenant switch
 * before the new session's data is allowed to render, and on a cross-tab session
 * change, so cleanup never depends on the component that opened the stream still
 * being mounted.
 */
export function closeAllViewSubscriptions(): void {
  for (const manager of registry) {
    manager.close();
  }
}

/**
 * Closes every open view stream and drops the subscribed views' cached data.
 */
export function isolateAllViewSubscriptions(): void {
  for (const manager of registry) {
    manager.closeAndIsolate();
  }
}

export function createViewSubscriptionManager(
  config: ViewSubscriptionManagerConfig,
): ViewSubscriptionManager {
  const {
    queryClient,
    transport,
    confirmTenant,
    reconnectDelaysMs = DEFAULT_RECONNECT_DELAYS_MS,
    staleAfterAttempts = DEFAULT_STALE_AFTER_ATTEMPTS,
    fallbackPollIntervalMs = DEFAULT_FALLBACK_POLL_INTERVAL_MS,
    ackBatchMs = DEFAULT_ACK_BATCH_MS,
    ack: customAck,
  } = config;
  const ackSender = customAck ?? (transport.ack ? transport.ack.bind(transport) : undefined);

  const listeners = new Set<() => void>();
  let state: ViewSubscriptionState = {
    status: "idle",
    lastUpdatedAt: null,
    lastResetReason: null,
  };

  let tenantId: string | null = null;
  let declared: readonly ViewSubscriptionDefinition[] = [];
  let subscribed: readonly ViewSubscriptionDefinition[] = [];
  let enabled = true;

  let handle: ViewStreamHandle | null = null;
  /** Retires in-flight opens and the handlers of any stream we have left behind. */
  let epoch = 0;
  /** The `tenant|views` identity of the stream that is open or being opened. */
  let openKey: string | null = null;
  let applied = new Map<string, AppliedRevision>();
  let currentStreamId: string | null = null;
  let pendingAcks = new Map<string, ViewAckItem>();
  let ackTimer: ReturnType<typeof setTimeout> | null = null;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let fallbackTimer: ReturnType<typeof setInterval> | null = null;

  function setState(next: Partial<ViewSubscriptionState>) {
    const merged = { ...state, ...next };
    if (
      merged.status === state.status &&
      merged.lastUpdatedAt === state.lastUpdatedAt &&
      merged.lastResetReason === state.lastResetReason
    ) {
      return;
    }
    state = merged;
    for (const listener of listeners) listener();
  }

  function rollingViews(views: readonly ViewSubscriptionDefinition[]) {
    return views.filter((view) => !isPinnedTimeRange(view.timeRange));
  }

  function clearReconnectTimer() {
    if (reconnectTimer !== null) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
  }

  function clearAckTimer() {
    if (ackTimer !== null) {
      clearTimeout(ackTimer);
      ackTimer = null;
    }
  }

  function flushAcks() {
    ackTimer = null;
    if (!ackSender || ackBatchMs <= 0 || pendingAcks.size === 0) {
      pendingAcks.clear();
      return;
    }
    const streamId = currentStreamId ?? "active";
    const batch: ViewAckItem[] = [];
    for (const [viewId, item] of pendingAcks.entries()) {
      const activeView = subscribed.find((candidate) => candidate.viewId === viewId);
      const activeApplied = applied.get(viewId);
      if (
        activeView &&
        activeApplied &&
        activeApplied.queryRevision === item.queryRevision &&
        activeApplied.revision !== null
      ) {
        batch.push(item);
      }
    }
    pendingAcks.clear();
    if (batch.length === 0) return;
    // Advisory flow-control signal only: failed or delayed ACKs must never
    // block rendering or affect the SSE lifecycle (PUSH-023-C2).
    void ackSender(streamId, batch).catch(() => {});
  }

  function scheduleAck(viewId: string, queryRevision: string, revision: string) {
    if (!ackSender || ackBatchMs <= 0) return;
    pendingAcks.set(viewId, { viewId, queryRevision, revision });
    if (ackTimer === null) {
      ackTimer = setTimeout(flushAcks, ackBatchMs);
    }
  }

  function stopFallbackPolling() {
    if (fallbackTimer !== null) {
      clearInterval(fallbackTimer);
      fallbackTimer = null;
    }
  }

  function startFallbackPolling() {
    if (fallbackTimer !== null) return;
    fallbackTimer = setInterval(() => {
      for (const view of subscribed) {
        void queryClient.invalidateQueries({ queryKey: view.queryKey });
      }
    }, fallbackPollIntervalMs);
  }

  /** Retires the current stream and any in-flight open. */
  function teardown() {
    epoch += 1;
    clearReconnectTimer();
    clearAckTimer();
    pendingAcks.clear();
    currentStreamId = null;
    stopFallbackPolling();
    handle?.close();
    handle = null;
    openKey = null;
    applied = new Map();
  }

  function dropCachedViewData(views: readonly ViewSubscriptionDefinition[]) {
    for (const view of views) {
      queryClient.removeQueries({ queryKey: view.queryKey });
    }
  }

  function openStream() {
    const generation = epoch;
    const views = subscribed;
    const forTenant = tenantId;
    openKey = `${forTenant}|${encodeViewsParam(views)}`;
    // A retry attempt must not downgrade a visible reconnecting/stale state back
    // to "connecting": the degraded indicator stays until the stream is live
    // again (PUSH-009).
    if (state.status !== "reconnecting" && state.status !== "stale") {
      setState({ status: "connecting" });
    }

    void confirmTenant().then(
      (confirmed) => {
        if (generation !== epoch) return;
        if (confirmed === null || confirmed !== forTenant) {
          openKey = null;
          setState({ status: "unauthorized" });
          return;
        }
        handle = transport.open(views, {
          onOpen: () => {
            if (generation !== epoch) return;
            reconnectAttempts = 0;
            stopFallbackPolling();
            setState({ status: "live" });
          },
          onStreamId: (streamId) => {
            if (generation !== epoch) return;
            currentStreamId = streamId;
          },
          onEvent: (event) => {
            if (generation !== epoch) return;
            applyEvent(event);
          },
          onError: (kind) => {
            if (generation !== epoch) return;
            handleStreamError(kind);
          },
        });
      },
      () => {
        if (generation !== epoch) return;
        setState({ status: "unauthorized" });
      },
    );
  }

  function reopen() {
    teardown();
    if (!enabled || subscribed.length === 0 || tenantId === null) return;
    openStream();
  }

  function handleStreamError(kind: ViewStreamErrorKind) {
    const generation = epoch;
    handle?.close();
    handle = null;
    reconnectAttempts += 1;

    if (reconnectAttempts >= staleAfterAttempts) {
      setState({ status: "stale" });
      startFallbackPolling();
    } else {
      setState({ status: "reconnecting" });
    }

    const scheduleRetry = () => {
      if (generation !== epoch) return;
      const delay =
        reconnectDelaysMs[Math.min(reconnectAttempts - 1, reconnectDelaysMs.length - 1)] ??
        DEFAULT_RECONNECT_DELAYS_MS[0];
      clearReconnectTimer();
      reconnectTimer = setTimeout(() => {
        reconnectTimer = null;
        if (generation !== epoch) return;
        openStream();
      }, delay);
    };

    if (kind === "fatal") {
      // The transport gave up, so the session has to be re-established as
      // authoritative before another attempt; an unauthorized session fails
      // closed rather than reconnect-looping.
      void confirmTenant().then(
        (confirmed) => {
          if (generation !== epoch) return;
          if (confirmed === null || confirmed !== tenantId) {
            clearReconnectTimer();
            stopFallbackPolling();
            openKey = null;
            setState({ status: "unauthorized" });
            return;
          }
          scheduleRetry();
        },
        () => {
          if (generation !== epoch) return;
          setState({ status: "unauthorized" });
        },
      );
      return;
    }
    scheduleRetry();
  }

  function applyEvent(event: ViewSubscriptionEvent) {
    if (event.type === "reset") {
      setState({ lastResetReason: event.reason });
      reconnectAttempts = 0;
      // Reopening the GET *is* re-registration; it yields a fresh authorized
      // snapshot. Cached data stays visible until that snapshot lands.
      reopen();
      return;
    }

    const view = subscribed.find((candidate) => candidate.viewId === event.viewId);
    if (!view) return;

    const previous = applied.get(view.viewId);
    if (previous && previous.queryRevision !== event.queryRevision) return;

    if (event.type === "invalidate") {
      void queryClient.invalidateQueries({ queryKey: view.queryKey });
      return;
    }

    if (previous?.revision != null) {
      const order = compareRevisions(event.revision, previous.revision);
      if (Number.isNaN(order) || order <= 0) return;
    }

    if (event.type === "replace" && event.supersedesThrough !== undefined) {
      const supOrder = compareRevisions(event.revision, event.supersedesThrough);
      if (Number.isNaN(supOrder) || supOrder < 0) return;
    }

    if (event.type === "delta") {
      const reducer = view.applyDelta ?? applyKeyedChanges;
      const next = reducer(queryClient.getQueryData(view.queryKey), event.changes);
      if (next === undefined) {
        // Nothing safe to patch (no snapshot yet, or an incompatible cached
        // shape): refetch authoritatively instead of guessing.
        void queryClient.invalidateQueries({ queryKey: view.queryKey });
        return;
      }
      queryClient.setQueryData(view.queryKey, next);
    } else if (view.applyData) {
      const next = view.applyData(queryClient.getQueryData(view.queryKey), event.data);
      if (next !== undefined) {
        queryClient.setQueryData(view.queryKey, next);
      }
    } else {
      queryClient.setQueryData(view.queryKey, event.data);
    }

    applied.set(view.viewId, {
      queryRevision: event.queryRevision,
      revision: event.revision,
    });
    scheduleAck(view.viewId, event.queryRevision, event.revision);
    setState({ lastUpdatedAt: Date.now() });
  }

  const manager: ViewSubscriptionManager = {
    getState: () => state,

    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    update({ tenantId: nextTenantId, views, enabled: nextEnabled = true }) {
      const tenantChanged = tenantId !== null && tenantId !== nextTenantId;
      if (tenantChanged) {
        // Isolate synchronously: the previous tenant's results must be gone
        // before anything can render under the new tenant.
        teardown();
        dropCachedViewData(subscribed);
        applied = new Map();
        reconnectAttempts = 0;
      }

      tenantId = nextTenantId;
      declared = views;
      subscribed = rollingViews(views);
      enabled = nextEnabled;

      if (!enabled || subscribed.length === 0) {
        teardown();
        setState({ status: declared.length > 0 && enabled ? "paused" : "idle" });
        return;
      }

      // An unchanged view set must never interrupt the stream: reconnects and
      // re-renders have to leave selection, filters, scroll and form state alone.
      if (!tenantChanged && openKey === `${tenantId}|${encodeViewsParam(subscribed)}`) return;

      teardown();
      openStream();
    },

    close() {
      teardown();
      reconnectAttempts = 0;
      setState({ status: "idle" });
    },

    closeAndIsolate() {
      teardown();
      dropCachedViewData(subscribed);
      reconnectAttempts = 0;
      setState({ status: "idle", lastUpdatedAt: null });
    },

    dispose() {
      this.close();
      registry.delete(manager);
    },
  };

  registry.add(manager);
  return manager;
}
