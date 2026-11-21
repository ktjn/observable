import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPlaygroundViewSubscriptions } from "./playgroundViewSubscriptions";
import type { PlaygroundViewSubscriptions } from "./playgroundViewSubscriptions";
import { createViewSubscriptionManager } from "./viewSubscriptionManager";
import type { ViewSubscriptionManager } from "./viewSubscriptionManager";
import type { ViewSubscriptionDefinition } from "./viewSubscriptionProtocol";
import type { LiveTickNotification } from "../playground/liveTick";

const TENANT_A = "tenant-a";
const TENANT_B = "tenant-b";
const HEALTH_KEY = ["view", "health"] as const;
const WATERMARK_KEY = ["view", "logs-watermark"] as const;
const TAIL_KEY = ["view", "logs-tail"] as const;

let liveTickListener: ((tick: LiveTickNotification) => void) | undefined;

vi.mock("../playground/engineClient", () => ({
  onLiveTick: (listener: (tick: LiveTickNotification) => void) => {
    liveTickListener = listener;
    return () => {
      liveTickListener = undefined;
    };
  },
}));

function fireLiveTick(tick: LiveTickNotification) {
  liveTickListener?.(tick);
}

function noopTick(overrides: Partial<LiveTickNotification> = {}): LiveTickNotification {
  return {
    type: "live-tick",
    nowMs: 1_700_000_000_000,
    logs: { watermarkNs: "1700000000000000000", count: 0 },
    spans: { watermarkNs: "1700000000000000000", count: 0 },
    metrics: { watermarkNs: "1700000000000000000", count: 0 },
    ...overrides,
  };
}

function healthView(): ViewSubscriptionDefinition {
  return {
    viewId: "health",
    query: { kind: "service-status" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: HEALTH_KEY,
  };
}

function logsWatermarkView(): ViewSubscriptionDefinition {
  return {
    viewId: "logs-watermark",
    query: { kind: "telemetry-watermark", signal: "logs" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: WATERMARK_KEY,
  };
}

function logsTailView(): ViewSubscriptionDefinition {
  return {
    viewId: "logs-tail",
    query: { kind: "log-tail" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: TAIL_KEY,
  };
}

/**
 * The fake is exercised through the real manager: a playground-mode component
 * must not be able to tell it apart from the `EventSource` transport (PUSH-010).
 */
describe("playground view subscription fake", () => {
  let queryClient: QueryClient;
  let fake: PlaygroundViewSubscriptions;
  let manager: ViewSubscriptionManager;

  async function settle() {
    await vi.advanceTimersByTimeAsync(0);
  }

  async function start(tenantId = TENANT_A, views: ViewSubscriptionDefinition[] = [healthView()]) {
    manager.update({ tenantId, views });
    await settle();
  }

  beforeEach(() => {
    vi.useFakeTimers();
    liveTickListener = undefined;
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    fake = createPlaygroundViewSubscriptions({ sessionTenantId: TENANT_A });
    fake.control.setSnapshotData("health", { payments: { health: "normal" } });
    manager = createViewSubscriptionManager({
      queryClient,
      transport: fake.transport,
      confirmTenant: fake.confirmTenant,
    });
  });

  afterEach(() => {
    manager.dispose();
    queryClient.clear();
    vi.useRealTimers();
    liveTickListener = undefined;
  });

  describe("success", () => {
    it("delivers an initial snapshot on connect", async () => {
      await start();
      expect(manager.getState().status).toBe("live");
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
    });

    it("reports the view definitions the connection registered", async () => {
      await start();
      expect(fake.control.openViews().map((v) => v.viewId)).toEqual(["health"]);
    });

    it("applies an enqueued delta", async () => {
      await start();
      fake.control.delta("health", [{ op: "upsert", key: "payments", value: { health: "degraded" } }]);
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
    });

    it("applies an enqueued replace", async () => {
      await start();
      fake.control.replace("health", { search: { health: "normal" } });
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ search: { health: "normal" } });
    });

    it("applies an enqueued invalidate as a refetch hint", async () => {
      await start();
      const invalidate = vi.spyOn(queryClient, "invalidateQueries");
      fake.control.invalidate("health");
      await settle();
      expect(invalidate).toHaveBeenCalledWith({ queryKey: HEALTH_KEY });
    });
  });

  describe("duplicate and out-of-order delivery", () => {
    it("a duplicated event never regresses the displayed result", async () => {
      await start();
      fake.control.replace("health", { payments: { health: "degraded" } });
      await settle();
      fake.control.resendLast();
      fake.control.resendLast();
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
    });

    it("a stale revision never regresses the displayed result", async () => {
      await start();
      fake.control.replace("health", { payments: { health: "degraded" } });
      await settle();
      fake.control.resendStale({ payments: { health: "from-the-past" } });
      await settle();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
    });

    it("an event for an unsubscribed view is ignored", async () => {
      await start();
      fake.control.replace("not-subscribed", { leaked: true });
      await settle();
      expect(queryClient.getQueryData(["view", "not-subscribed"])).toBeUndefined();
    });
  });

  describe("gap and reset", () => {
    it("a reset re-registers the same views and yields a fresh snapshot", async () => {
      await start();
      fake.control.replace("health", { payments: { health: "degraded" } });
      await settle();
      fake.control.setSnapshotData("health", { payments: { health: "resynchronized" } });

      fake.control.reset("cursor_unavailable");
      await settle();

      expect(fake.control.openCount()).toBe(2);
      expect(manager.getState().lastResetReason).toBe("cursor_unavailable");
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({
        payments: { health: "resynchronized" },
      });
    });
  });

  describe("disconnect and reconnect", () => {
    it("a dropped connection reconnects and returns to live", async () => {
      await start();
      fake.control.disconnect();
      expect(manager.getState().status).toBe("reconnecting");
      await vi.advanceTimersByTimeAsync(1_000);
      expect(manager.getState().status).toBe("live");
      expect(fake.control.openCount()).toBe(2);
    });

    it("keeps the previously authorized result visible while reconnecting", async () => {
      await start();
      fake.control.replace("health", { payments: { health: "degraded" } });
      await settle();
      fake.control.disconnect();
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
    });

    it("repeated failures to connect surface a stale view", async () => {
      await start();
      fake.control.failOpens(20);
      fake.control.disconnect();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(manager.getState().status).toBe("stale");
    });
  });

  describe("forbidden session", () => {
    it("a revoked session fails closed instead of reconnecting", async () => {
      await start();
      fake.control.revokeSession();
      fake.control.disconnect("fatal");
      await settle();
      expect(manager.getState().status).toBe("unauthorized");
      expect(fake.control.openCount()).toBe(1);
    });
  });

  describe("tenant switch", () => {
    it("a switch closes the old stream, drops its data and opens an authorized one", async () => {
      await start(TENANT_A);
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });

      fake.control.switchTenant(TENANT_B);
      manager.update({ tenantId: TENANT_B, views: [healthView()] });
      expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();

      fake.control.setSnapshotData("health", { billing: { health: "normal" } });
      await settle();
      expect(fake.control.openCount()).toBe(2);
      expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ billing: { health: "normal" } });
    });

    it("notifies listeners so another tab's switch can be reconciled", async () => {
      const listener = vi.fn();
      const unsubscribe = fake.control.onTenantChange(listener);
      await start(TENANT_A);
      fake.control.switchTenant(TENANT_B);
      expect(listener).toHaveBeenCalledWith(TENANT_B);
      unsubscribe();
    });

    it("refuses to stream the old tenant after the session switched", async () => {
      await start(TENANT_A);
      fake.control.switchTenant(TENANT_B);
      fake.control.disconnect("fatal");
      await settle();
      expect(manager.getState().status).toBe("unauthorized");
    });
  });

  describe("control reset", () => {
    it("clears scripted state between scenarios", async () => {
      await start();
      fake.control.clear();
      expect(fake.control.openCount()).toBe(0);
      expect(fake.control.isConnected()).toBe(false);
    });
  });

  /**
   * ADR-047 §2: liveTick's worker notifications must reach subscribers
   * through this same push-protocol fake, not a separate polling path.
   */
  describe("live-tick bridge", () => {
    it("replaces a telemetry-watermark view's data on every tick", async () => {
      await start(TENANT_A, [logsWatermarkView()]);
      fireLiveTick(noopTick({ logs: { watermarkNs: "1700000000000000000", count: 5 } }));
      await settle();
      expect(queryClient.getQueryData(WATERMARK_KEY)).toEqual({
        watermark_unix_nano: 1700000000000000000,
        coalesced_records: 5,
      });
    });

    it("ignores a telemetry-watermark view subscribed to a different signal", async () => {
      await start(TENANT_A, [
        {
          viewId: "spans-watermark",
          query: { kind: "telemetry-watermark", signal: "metrics" },
          timeRange: { mode: "rolling", window: "15m" },
          queryKey: ["view", "spans-watermark"] as const,
        },
      ]);
      fireLiveTick(noopTick({ metrics: { watermarkNs: "1700000000000000000", count: 7 } }));
      await settle();
      expect(queryClient.getQueryData(["view", "spans-watermark"])).toEqual({
        watermark_unix_nano: 1700000000000000000,
        coalesced_records: 7,
      });
    });

    it("delivers a log-tail delta only when the tick produced new logs", async () => {
      fake.control.setSnapshotData("logs-tail", {});
      await start(TENANT_A, [logsTailView()]);
      fireLiveTick(noopTick({ logs: { watermarkNs: "1700000000000000000", count: 0 } }));
      await settle();
      expect(queryClient.getQueryData(TAIL_KEY)).toEqual({});

      fireLiveTick(noopTick({ logs: { watermarkNs: "1700000000000001000", count: 3 } }));
      await settle();
      expect(queryClient.getQueryData(TAIL_KEY)).toEqual({
        "watermark:1700000000000001000": { watermark_unix_nano: 1700000000000001000, coalesced_records: 3 },
      });
    });

    it("does nothing while disconnected", async () => {
      await start(TENANT_A, [logsWatermarkView()]);
      fake.control.disconnect();
      expect(() => fireLiveTick(noopTick())).not.toThrow();
      await settle();
      expect(queryClient.getQueryData(WATERMARK_KEY)).toBeUndefined();
    });

    it("never calls onLiveTick before any connection has opened", () => {
      expect(liveTickListener).toBeUndefined();
    });
  });
});
