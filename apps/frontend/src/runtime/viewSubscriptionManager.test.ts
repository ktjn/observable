import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  closeAllViewSubscriptions,
  createViewSubscriptionManager,
} from "./viewSubscriptionManager";
import type { ViewSubscriptionManager } from "./viewSubscriptionManager";
import type {
  ViewStreamHandle,
  ViewStreamHandlers,
  ViewSubscriptionDefinition,
  ViewSubscriptionEvent,
  ViewSubscriptionTransport,
} from "./viewSubscriptionProtocol";

const TENANT_A = "tenant-a";
const TENANT_B = "tenant-b";

const HEALTH_KEY = ["view", "health"] as const;
const STATUS_KEY = ["view", "status"] as const;

function healthView(): ViewSubscriptionDefinition {
  return {
    viewId: "health",
    query: {
      kind: "service-health",
      serviceId: "11111111-1111-4111-8111-111111111111",
      environmentClass: "production",
    },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: HEALTH_KEY,
  };
}

function statusView(): ViewSubscriptionDefinition {
  return {
    viewId: "status",
    query: { kind: "service-status" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: STATUS_KEY,
  };
}

function pinnedHealthView(): ViewSubscriptionDefinition {
  return { ...healthView(), timeRange: { mode: "fixed", from: 1, to: 2 } };
}

interface OpenedStream {
  views: readonly ViewSubscriptionDefinition[];
  handlers: ViewStreamHandlers;
  closed: boolean;
}

/** Scripted transport: the seam the manager is tested through. */
function fakeTransport() {
  const opened: OpenedStream[] = [];
  const transport: ViewSubscriptionTransport = {
    open(views, handlers): ViewStreamHandle {
      const stream: OpenedStream = { views, handlers, closed: false };
      opened.push(stream);
      return {
        close() {
          stream.closed = true;
        },
      };
    },
  };
  return {
    transport,
    opened,
    get current(): OpenedStream {
      const live = opened[opened.length - 1];
      if (!live) throw new Error("no stream has been opened");
      return live;
    },
    openCount: () => opened.length,
  };
}

function snapshot(revision: string, data: unknown, queryRevision = "r1"): ViewSubscriptionEvent {
  return { type: "snapshot", viewId: "health", queryRevision, revision, data };
}

describe("view subscription manager", () => {
  let queryClient: QueryClient;
  let transport: ReturnType<typeof fakeTransport>;
  let confirmTenant: ReturnType<typeof vi.fn>;
  let manager: ViewSubscriptionManager;

  function build(overrides: { confirmTenant?: () => Promise<string | null> } = {}) {
    return createViewSubscriptionManager({
      queryClient,
      transport: transport.transport,
      confirmTenant: overrides.confirmTenant ?? (confirmTenant as () => Promise<string | null>),
    });
  }

  /** Lets the injected `confirmTenant` promise settle before opening. */
  async function settle() {
    await vi.advanceTimersByTimeAsync(0);
  }

  async function startWith(views: ViewSubscriptionDefinition[], tenantId = TENANT_A) {
    manager.update({ tenantId, views });
    await settle();
  }

  beforeEach(() => {
    vi.useFakeTimers();
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    transport = fakeTransport();
    confirmTenant = vi.fn(async () => TENANT_A);
    manager = build();
  });

  afterEach(() => {
    manager.close();
    closeAllViewSubscriptions();
    queryClient.clear();
    vi.useRealTimers();
  });

  describe("registration", () => {
    it("starts idle with no views", () => {
      expect(manager.getState().status).toBe("idle");
      expect(transport.openCount()).toBe(0);
    });

    it("opens one stream carrying every rolling view definition", async () => {
      await startWith([healthView(), statusView()]);
      expect(transport.openCount()).toBe(1);
      expect(transport.current.views.map((v) => v.viewId)).toEqual(["health", "status"]);
    });

    it("reports live once the stream is open", async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      expect(manager.getState().status).toBe("live");
    });

    it("does not reopen the stream when the same view set is re-declared", async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      manager.update({ tenantId: TENANT_A, views: [healthView()] });
      await settle();
      expect(transport.openCount()).toBe(1);
      expect(manager.getState().status).toBe("live");
    });

    it("closes the old stream and opens a new one when a filter changes", async () => {
      await startWith([statusView()]);
      const first = transport.current;
      manager.update({
        tenantId: TENANT_A,
        views: [{ ...statusView(), query: { kind: "service-status", environment: "staging" } }],
      });
      await settle();
      expect(first.closed).toBe(true);
      expect(transport.openCount()).toBe(2);
    });

    it("preserves already-applied view data across a view-set update", async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "normal" } }));
      manager.update({ tenantId: TENANT_A, views: [healthView(), statusView()] });
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("notifies subscribers on status transitions", async () => {
      const listener = vi.fn();
      const unsubscribe = manager.subscribe(listener);
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      expect(listener).toHaveBeenCalled();
      unsubscribe();
    });
  });

  describe("pinned versus rolling views", () => {
    it("does not subscribe a pinned historical view and reports paused", async () => {
      await startWith([pinnedHealthView()]);
      expect(transport.openCount()).toBe(0);
      expect(manager.getState().status).toBe("paused");
    });

    it("subscribes only the rolling views of a mixed set", async () => {
      await startWith([pinnedHealthView(), statusView()]);
      expect(transport.current.views.map((v) => v.viewId)).toEqual(["status"]);
    });

    it("leaves a pinned view's cached data untouched when its view id is pushed", async () => {
      queryClient.setQueryData(HEALTH_KEY, { payments: { health: "normal" } });
      await startWith([pinnedHealthView(), statusView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("9", { payments: { health: "degraded" } }));
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });
  });

  describe("applying data events", () => {
    beforeEach(async () => {
      await startWith([healthView(), statusView()]);
      transport.current.handlers.onOpen();
    });

    it("applies a snapshot straight to the view's query key", () => {
      transport.current.handlers.onEvent(snapshot("1842", { payments: { health: "normal" } }));
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
      expect(manager.getState().lastUpdatedAt).not.toBeNull();
    });

    it("folds a delta's keyed ops into the cached snapshot", () => {
      transport.current.handlers.onEvent(
        snapshot("1842", { payments: { health: "normal" }, search: { health: "normal" } }),
      );
      transport.current.handlers.onEvent({
        type: "delta",
        viewId: "health",
        queryRevision: "r1",
        revision: "1843",
        changes: [{ op: "upsert", key: "payments", value: { health: "degraded" } }],
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({
        payments: { health: "degraded" },
        search: { health: "normal" },
      });
    });

    it("applies a delete op", () => {
      transport.current.handlers.onEvent(snapshot("1", { payments: {}, search: {} }));
      transport.current.handlers.onEvent({
        type: "delta",
        viewId: "health",
        queryRevision: "r1",
        revision: "2",
        changes: [{ op: "delete", key: "search" }],
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: {} });
    });

    it("replaces the whole cached result on replace", () => {
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "normal" } }));
      transport.current.handlers.onEvent({
        type: "replace",
        viewId: "health",
        queryRevision: "r1",
        revision: "2",
        data: { search: { health: "degraded" } },
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ search: { health: "degraded" } });
    });

    it("leaves an unrelated view's query key untouched", () => {
      queryClient.setQueryData(STATUS_KEY, { untouched: true });
      transport.current.handlers.onEvent(snapshot("1", { payments: {} }));
      expect(queryClient.getQueryData(STATUS_KEY)).toEqual({ untouched: true });
    });

    it("ignores an event for a view it is not subscribed to", () => {
      transport.current.handlers.onEvent({
        type: "snapshot",
        viewId: "not-subscribed",
        queryRevision: "r1",
        revision: "1",
        data: { leaked: true },
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
      expect(queryClient.getQueryData(STATUS_KEY)).toBeUndefined();
    });

    it("applies data without refetching the view's query", () => {
      const fetchSpy = vi.spyOn(queryClient, "invalidateQueries");
      transport.current.handlers.onEvent(snapshot("1", { payments: {} }));
      expect(fetchSpy).not.toHaveBeenCalled();
    });
  });

  describe("idempotent revision handling", () => {
    beforeEach(async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("1842", { payments: { health: "normal" } }));
    });

    it("ignores a duplicate revision", () => {
      transport.current.handlers.onEvent(snapshot("1842", { payments: { health: "duplicate" } }));
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("ignores an out-of-order stale revision", () => {
      transport.current.handlers.onEvent({
        type: "replace",
        viewId: "health",
        queryRevision: "r1",
        revision: "1841",
        data: { payments: { health: "stale" } },
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("accepts a strictly newer revision", () => {
      transport.current.handlers.onEvent({
        type: "replace",
        viewId: "health",
        queryRevision: "r1",
        revision: "1843",
        data: { payments: { health: "degraded" } },
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
    });

    it("ignores a result belonging to an obsolete view-query revision", () => {
      transport.current.handlers.onEvent(
        snapshot("9999", { payments: { health: "other-definition" } }, "r-obsolete"),
      );
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("ignores events from a stream it has already replaced", async () => {
      const stale = transport.current;
      manager.update({ tenantId: TENANT_A, views: [healthView(), statusView()] });
      await settle();
      stale.handlers.onEvent({
        type: "replace",
        viewId: "health",
        queryRevision: "r1",
        revision: "99999",
        data: { payments: { health: "from-closed-stream" } },
      });
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("accepts a fresh snapshot after a reopen even though its revision restarts", async () => {
      transport.current.handlers.onEvent({ type: "reset", reason: "cursor_unavailable" });
      await settle();
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "fresh" } }, "r2"));
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "fresh" } });
    });
  });

  describe("invalidate fallback", () => {
    beforeEach(async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
    });

    it("refetches the view's query on an invalidate hint", () => {
      const invalidate = vi.spyOn(queryClient, "invalidateQueries");
      transport.current.handlers.onEvent({
        type: "invalidate",
        viewId: "health",
        queryRevision: "r1",
      });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: HEALTH_KEY });
    });

    it("falls back to an authoritative refetch for a delta it cannot fold in", () => {
      const invalidate = vi.spyOn(queryClient, "invalidateQueries");
      transport.current.handlers.onEvent({
        type: "delta",
        viewId: "health",
        queryRevision: "r1",
        revision: "5",
        changes: [{ op: "upsert", key: "payments", value: {} }],
      });
      expect(invalidate).toHaveBeenCalledWith({ queryKey: HEALTH_KEY });
    });

    it("uses a view's own delta reducer when it supplies one", () => {
      manager.update({
        tenantId: TENANT_A,
        views: [
          {
            ...healthView(),
            applyDelta: (current, changes) => ({
              rows: [...((current as { rows?: string[] } | undefined)?.rows ?? []), ...changes.map((c) => c.key)],
            }),
          },
        ],
      });
      return settle().then(() => {
        transport.current.handlers.onOpen();
        transport.current.handlers.onEvent({
          type: "delta",
          viewId: "health",
          queryRevision: "r1",
          revision: "5",
          changes: [{ op: "upsert", key: "payments", value: {} }],
        });
        expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ rows: ["payments"] });
      });
    });
  });

  describe("reset", () => {
    beforeEach(async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
    });

    it("reopens the stream with the same view definitions to get a fresh snapshot", async () => {
      const first = transport.current;
      first.handlers.onEvent({ type: "reset", reason: "cursor_unavailable" });
      await settle();
      expect(first.closed).toBe(true);
      expect(transport.openCount()).toBe(2);
      expect(transport.current.views.map((v) => v.viewId)).toEqual(["health"]);
    });

    it("records the reset reason", async () => {
      transport.current.handlers.onEvent({ type: "reset", reason: "subscriber_lag" });
      await settle();
      expect(manager.getState().lastResetReason).toBe("subscriber_lag");
    });

    it("keeps the previously authorized data visible while re-registering", async () => {
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "normal" } }));
      transport.current.handlers.onEvent({ type: "reset", reason: "cursor_unavailable" });
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });
  });

  describe("reconnect, stale and fallback polling", () => {
    beforeEach(async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
    });

    it("reports reconnecting after a transient transport failure", () => {
      transport.current.handlers.onError("transient");
      expect(manager.getState().status).toBe("reconnecting");
    });

    it("reopens the stream after a backoff delay", async () => {
      transport.current.handlers.onError("transient");
      expect(transport.openCount()).toBe(1);
      await vi.advanceTimersByTimeAsync(1_000);
      expect(transport.openCount()).toBe(2);
    });

    it("returns to live when the reopened stream connects", async () => {
      transport.current.handlers.onError("transient");
      await vi.advanceTimersByTimeAsync(1_000);
      transport.current.handlers.onOpen();
      expect(manager.getState().status).toBe("live");
    });

    it("becomes stale after repeated failed reconnects", async () => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        transport.current.handlers.onError("transient");
        await vi.advanceTimersByTimeAsync(60_000);
      }
      expect(manager.getState().status).toBe("stale");
    });

    it("falls back to bounded polling of the view's query while stale", async () => {
      const invalidate = vi.spyOn(queryClient, "invalidateQueries");
      for (let attempt = 0; attempt < 3; attempt += 1) {
        transport.current.handlers.onError("transient");
        await vi.advanceTimersByTimeAsync(60_000);
      }
      invalidate.mockClear();
      await vi.advanceTimersByTimeAsync(30_000);
      expect(invalidate).toHaveBeenCalledWith({ queryKey: HEALTH_KEY });
    });

    it("stops fallback polling once the stream recovers", async () => {
      for (let attempt = 0; attempt < 3; attempt += 1) {
        transport.current.handlers.onError("transient");
        await vi.advanceTimersByTimeAsync(60_000);
      }
      transport.current.handlers.onOpen();
      const invalidate = vi.spyOn(queryClient, "invalidateQueries");
      await vi.advanceTimersByTimeAsync(120_000);
      expect(invalidate).not.toHaveBeenCalled();
      expect(manager.getState().status).toBe("live");
    });

    it("does not reconnect after close", async () => {
      transport.current.handlers.onError("transient");
      manager.close();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(transport.openCount()).toBe(1);
      expect(manager.getState().status).toBe("idle");
    });
  });

  describe("authorization", () => {
    it("reports unauthorized and stops when the session no longer confirms the tenant", async () => {
      manager = build({ confirmTenant: async () => null });
      await startWith([healthView()]);
      expect(transport.openCount()).toBe(0);
      expect(manager.getState().status).toBe("unauthorized");
    });

    it("does not open a stream for a tenant the session disagrees with", async () => {
      manager = build({ confirmTenant: async () => TENANT_B });
      await startWith([healthView()], TENANT_A);
      expect(transport.openCount()).toBe(0);
      expect(manager.getState().status).toBe("unauthorized");
    });

    it("reports unauthorized when a fatal transport failure is not a session problem it can retry", async () => {
      const confirm = vi
        .fn<() => Promise<string | null>>()
        .mockResolvedValueOnce(TENANT_A)
        .mockResolvedValue(null);
      manager = build({ confirmTenant: confirm });
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onError("fatal");
      await settle();
      expect(manager.getState().status).toBe("unauthorized");
      expect(transport.openCount()).toBe(1);
    });

    it("reconnects after a fatal failure that the session still authorizes", async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onError("fatal");
      await vi.advanceTimersByTimeAsync(1_000);
      expect(transport.openCount()).toBe(2);
    });

    it("confirms the authoritative session tenant before opening any stream", async () => {
      await startWith([healthView()]);
      expect(confirmTenant).toHaveBeenCalled();
    });
  });

  describe("tenant switch and logout", () => {
    it("closes the stream and drops the subscribed views' cached data", async () => {
      await startWith([healthView(), statusView()]);
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "normal" } }));
      queryClient.setQueryData(STATUS_KEY, { tenantA: true });

      manager.closeAndIsolate();

      expect(transport.current.closed).toBe(true);
      expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
      expect(queryClient.getQueryData(STATUS_KEY)).toBeUndefined();
      expect(manager.getState().status).toBe("idle");
    });

    it("isolates the previous tenant's view data before the new tenant can render", async () => {
      await startWith([healthView()], TENANT_A);
      transport.current.handlers.onOpen();
      transport.current.handlers.onEvent(snapshot("1", { payments: { health: "tenant-a" } }));

      confirmTenant.mockResolvedValue(TENANT_B);
      manager.update({ tenantId: TENANT_B, views: [healthView()] });
      expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
      await settle();
      expect(transport.openCount()).toBe(2);
    });

    it("does not open the new tenant's stream until the session confirms the switch", async () => {
      await startWith([healthView()], TENANT_A);
      const streamA = transport.current;
      streamA.handlers.onOpen();

      let confirmSwitch: (tenantId: string) => void = () => {};
      confirmTenant.mockImplementation(
        () => new Promise<string>((resolve) => { confirmSwitch = resolve; }),
      );
      manager.update({ tenantId: TENANT_B, views: [healthView()] });
      await settle();
      expect(streamA.closed).toBe(true);
      expect(transport.openCount()).toBe(1);

      confirmSwitch(TENANT_B);
      await settle();
      expect(transport.openCount()).toBe(2);
    });

    it("drops an old-tenant event that arrives after the switch started", async () => {
      await startWith([healthView()], TENANT_A);
      const streamA = transport.current;
      streamA.handlers.onOpen();
      confirmTenant.mockResolvedValue(TENANT_B);
      manager.update({ tenantId: TENANT_B, views: [healthView()] });

      streamA.handlers.onEvent(snapshot("5", { payments: { health: "tenant-a-leak" } }));
      expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
    });

    it("closes every registered manager on a global close", async () => {
      await startWith([healthView()]);
      transport.current.handlers.onOpen();
      closeAllViewSubscriptions();
      expect(transport.current.closed).toBe(true);
      expect(manager.getState().status).toBe("idle");
    });
  });
});
