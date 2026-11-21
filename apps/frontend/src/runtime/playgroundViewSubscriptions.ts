import type {
  ViewAckItem,
  ViewChange,
  ViewStreamErrorKind,
  ViewStreamHandle,
  ViewStreamHandlers,
  ViewSubscriptionDefinition,
  ViewSubscriptionEvent,
  ViewSubscriptionTransport,
} from "./viewSubscriptionProtocol";
import type { LiveTickNotification } from "../playground/liveTick";

/**
 * The browser-only playground's programmable stand-in for
 * `GET /v1/ui/subscriptions/events` (PUSH-010).
 *
 * It implements the same `ViewSubscriptionTransport` the production
 * `EventSource` adapter does, so the view-subscription manager — and therefore
 * every consuming component — behaves identically in playground mode. Nothing
 * here touches the network: scenarios are driven entirely through `control`,
 * which can script success, duplicate delivery, stale revisions, replay gaps,
 * disconnects, a forbidden session and a tenant switch.
 */

export interface PlaygroundViewSubscriptionControl {
  // ── Session ────────────────────────────────────────────────────────────────
  /** The tenant the simulated session currently authorizes. */
  sessionTenantId(): string;
  /**
   * Simulates the session's active tenant changing — including from another tab.
   * Does not itself drop the stream; call `disconnect("fatal")` to simulate the
   * server terminating the now-obsolete stream.
   */
  switchTenant(tenantId: string): void;
  onTenantChange(listener: (tenantId: string) => void): () => void;
  /** Makes session confirmation fail closed, as a revoked membership would. */
  revokeSession(): void;
  restoreSession(): void;

  // ── Connection ─────────────────────────────────────────────────────────────
  /** Total connections opened since the last `clear()`. */
  openCount(): number;
  isConnected(): boolean;
  /** The view definitions the live connection registered. */
  openViews(): readonly ViewSubscriptionDefinition[];
  /** Whether a new connection reports itself open (and snapshots) by itself. */
  setAutoConnect(auto: boolean): void;
  /** Reports the live connection as open, delivering any scripted snapshots. */
  connect(): void;
  /** Drops the live connection. */
  disconnect(kind?: ViewStreamErrorKind): void;
  /** Fails the next `count` connection attempts without ever connecting. */
  failOpens(count: number, kind?: ViewStreamErrorKind): void;

  // ── Events ─────────────────────────────────────────────────────────────────
  /** The snapshot a view is served when a connection opens. */
  setSnapshotData(viewId: string, data: unknown): void;
  emit(event: ViewSubscriptionEvent): void;
  snapshot(viewId: string, data: unknown): void;
  delta(viewId: string, changes: ViewChange[]): void;
  replace(viewId: string, data: unknown): void;
  /**
   * Emits a lag-conflated replacement advancing revision past `skippedDeltas`
   * intermediate updates (`supersedesThrough`, PUSH-021).
   */
  replaceSuperseding(
    viewId: string,
    data: unknown,
    skippedDeltas?: number,
    reason?: string,
  ): void;
  invalidate(viewId: string): void;
  reset(reason?: string): void;
  /** Re-delivers the last data event verbatim (at-least-once duplication). */
  resendLast(): void;
  /** Re-delivers the last data event behind its revision (out-of-order). */
  resendStale(data?: unknown): void;
  /** Batches of applied-revision ACKs reported by the browser (PUSH-023). */
  ackedBatches(): readonly (readonly ViewAckItem[])[];

  /** Forgets all scripted state and closes any live connection. */
  clear(): void;
}

export interface PlaygroundViewSubscriptions {
  transport: ViewSubscriptionTransport;
  confirmTenant: () => Promise<string | null>;
  control: PlaygroundViewSubscriptionControl;
}

interface Connection {
  views: readonly ViewSubscriptionDefinition[];
  handlers: ViewStreamHandlers;
  queryRevision: string;
  closed: boolean;
  opened: boolean;
}

const FIRST_REVISION = 1_000;

export function createPlaygroundViewSubscriptions(
  options: { sessionTenantId: string } = { sessionTenantId: "00000000-0000-0000-0000-000000000001" },
): PlaygroundViewSubscriptions {
  const initialTenantId = options.sessionTenantId;

  let sessionTenantId = initialTenantId;
  let sessionRevoked = false;
  const tenantListeners = new Set<(tenantId: string) => void>();

  let connection: Connection | null = null;
  let openCount = 0;
  let autoConnect = true;
  let pendingOpenFailures = 0;
  let pendingOpenFailureKind: ViewStreamErrorKind = "transient";

  let revision = FIRST_REVISION;
  const snapshots = new Map<string, unknown>();
  let lastDataEvent: ViewSubscriptionEvent | null = null;
  const recordedAcks: (readonly ViewAckItem[])[] = [];

  /** Deferred so a connection's first events never re-enter `open()`. */
  function deliver(target: Connection, event: ViewSubscriptionEvent) {
    queueMicrotask(() => {
      if (target.closed) return;
      target.handlers.onEvent(event);
    });
  }

  function nextRevision(): string {
    revision += 1;
    return String(revision);
  }

  function emitTo(target: Connection, event: ViewSubscriptionEvent) {
    if (event.type !== "reset" && event.type !== "invalidate") {
      lastDataEvent = event;
    }
    deliver(target, event);
  }

  function requireConnection(): Connection {
    if (connection === null || connection.closed) {
      throw new Error("playground view subscription fake has no live connection");
    }
    return connection;
  }

  /**
   * ADR-047 §2: liveTick's worker notifications arrive here, the same way
   * a real `telemetry.watermarks.v1` provider event would reach the
   * broker's `events.rs` reaction table — matched against every
   * currently-subscribed view, unscoped (no per-service filtering; see
   * ADR-047-wasm-local-observability-engine.md's §2 for why).
   */
  function reactToLiveTick(tick: LiveTickNotification) {
    if (connection === null || connection.closed) return;
    for (const view of connection.views) {
      if (view.query.kind === "telemetry-watermark") {
        const bucket = tick[view.query.signal];
        emitTo(connection, {
          type: "replace",
          viewId: view.viewId,
          queryRevision: connection.queryRevision,
          revision: nextRevision(),
          data: { watermark_unix_nano: Number(bucket.watermarkNs), coalesced_records: bucket.count },
        });
      } else if (view.query.kind === "log-tail" && tick.logs.count > 0) {
        emitTo(connection, {
          type: "delta",
          viewId: view.viewId,
          queryRevision: connection.queryRevision,
          revision: nextRevision(),
          changes: [
            {
              op: "upsert",
              key: `watermark:${tick.logs.watermarkNs}`,
              value: { watermark_unix_nano: Number(tick.logs.watermarkNs), coalesced_records: tick.logs.count },
            },
          ],
        });
      }
    }
  }

  // Wired lazily on the first `transport.open()` rather than at factory
  // construction: this module is statically imported in both runtime
  // modes (ADR-047 §1 — httpRuntime/playgroundRuntime are both always
  // bundled), but `open()` is only ever called when the app is actually
  // running in playground mode. Wiring here, not at module load, keeps
  // the dynamic `engineClient` import (and the WASM engine it starts)
  // out of hosted mode entirely.
  let liveTickBridgeStarted = false;
  function startLiveTickBridge() {
    if (liveTickBridgeStarted) return;
    liveTickBridgeStarted = true;
    void import("../playground/engineClient")
      .then(({ onLiveTick }) => {
        onLiveTick(reactToLiveTick);
      })
      .catch(() => {
        // Best-effort: the engine may be unavailable (no Worker support,
        // or a test environment with no engineClient mock). View
        // subscriptions still work via snapshots/control-driven events
        // either way; only the push-on-tick behavior is lost. Let a
        // later open() retry rather than disabling this permanently.
        liveTickBridgeStarted = false;
      });
  }

  function openConnection(target: Connection) {
    if (target.closed || target.opened) return;
    target.opened = true;
    target.handlers.onStreamId?.(`playground-stream-${openCount}`);
    target.handlers.onOpen();
    for (const view of target.views) {
      if (!snapshots.has(view.viewId)) continue;
      emitTo(target, {
        type: "snapshot",
        viewId: view.viewId,
        queryRevision: target.queryRevision,
        revision: nextRevision(),
        data: snapshots.get(view.viewId),
      });
    }
  }

  const transport: ViewSubscriptionTransport = {
    open(views, handlers): ViewStreamHandle {
      startLiveTickBridge();
      openCount += 1;
      const target: Connection = {
        views,
        handlers,
        queryRevision: `r${openCount}`,
        closed: false,
        opened: false,
      };
      connection = target;

      if (pendingOpenFailures > 0) {
        pendingOpenFailures -= 1;
        const kind = pendingOpenFailureKind;
        queueMicrotask(() => {
          if (!target.closed) target.handlers.onError(kind);
        });
      } else if (autoConnect) {
        queueMicrotask(() => openConnection(target));
      }

      return {
        close() {
          target.closed = true;
          if (connection === target) connection = null;
        },
      };
    },

    async ack(_streamId, acks) {
      recordedAcks.push([...acks]);
    },
  };

  const control: PlaygroundViewSubscriptionControl = {
    sessionTenantId: () => sessionTenantId,

    switchTenant(tenantId) {
      sessionTenantId = tenantId;
      for (const listener of tenantListeners) listener(tenantId);
    },

    onTenantChange(listener) {
      tenantListeners.add(listener);
      return () => tenantListeners.delete(listener);
    },

    revokeSession() {
      sessionRevoked = true;
    },

    restoreSession() {
      sessionRevoked = false;
    },

    openCount: () => openCount,
    isConnected: () => connection !== null && !connection.closed,
    openViews: () => connection?.views ?? [],

    setAutoConnect(auto) {
      autoConnect = auto;
    },

    connect() {
      openConnection(requireConnection());
    },

    disconnect(kind = "transient") {
      const target = requireConnection();
      target.closed = true;
      connection = null;
      target.handlers.onError(kind);
    },

    failOpens(count, kind = "transient") {
      pendingOpenFailures = count;
      pendingOpenFailureKind = kind;
    },

    setSnapshotData(viewId, data) {
      snapshots.set(viewId, data);
    },

    emit(event) {
      emitTo(requireConnection(), event);
    },

    snapshot(viewId, data) {
      const target = requireConnection();
      emitTo(target, {
        type: "snapshot",
        viewId,
        queryRevision: target.queryRevision,
        revision: nextRevision(),
        data,
      });
    },

    delta(viewId, changes) {
      const target = requireConnection();
      emitTo(target, {
        type: "delta",
        viewId,
        queryRevision: target.queryRevision,
        revision: nextRevision(),
        changes,
      });
    },

    replace(viewId, data) {
      const target = requireConnection();
      emitTo(target, {
        type: "replace",
        viewId,
        queryRevision: target.queryRevision,
        revision: nextRevision(),
        data,
      });
    },

    replaceSuperseding(viewId, data, skippedDeltas = 2, reason = "subscriber_lag") {
      const target = requireConnection();
      revision += Math.max(1, skippedDeltas);
      const supersedesThrough = String(revision);
      const replacementRevision = nextRevision();
      emitTo(target, {
        type: "replace",
        viewId,
        queryRevision: target.queryRevision,
        revision: replacementRevision,
        supersedesThrough,
        reason,
        data,
      });
    },

    invalidate(viewId) {
      const target = requireConnection();
      deliver(target, {
        type: "invalidate",
        viewId,
        queryRevision: target.queryRevision,
      });
    },

    reset(reason = "cursor_unavailable") {
      deliver(requireConnection(), { type: "reset", reason });
    },

    resendLast() {
      if (lastDataEvent === null) return;
      deliver(requireConnection(), lastDataEvent);
    },

    resendStale(data) {
      if (lastDataEvent === null || lastDataEvent.type === "reset") return;
      if (lastDataEvent.type === "invalidate") return;
      const stale = String(Math.max(0, Number(lastDataEvent.revision) - 1));
      deliver(requireConnection(), {
        ...lastDataEvent,
        revision: stale,
        ...(data === undefined ? {} : { data }),
      } as ViewSubscriptionEvent);
    },

    ackedBatches: () => recordedAcks,

    clear() {
      if (connection !== null) connection.closed = true;
      connection = null;
      openCount = 0;
      autoConnect = true;
      pendingOpenFailures = 0;
      pendingOpenFailureKind = "transient";
      revision = FIRST_REVISION;
      snapshots.clear();
      lastDataEvent = null;
      recordedAcks.length = 0;
      sessionTenantId = initialTenantId;
      sessionRevoked = false;
    },
  };

  return {
    transport,
    confirmTenant: async () => (sessionRevoked ? null : sessionTenantId),
    control,
  };
}

/** The playground runtime's single fake, shared by every playground-mode view. */
export const playgroundViewSubscriptions = createPlaygroundViewSubscriptions();
