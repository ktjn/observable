import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createPlaygroundViewSubscriptions } from "./playgroundViewSubscriptions";
import type { PlaygroundViewSubscriptions } from "./playgroundViewSubscriptions";
import { createViewSubscriptionManager } from "./viewSubscriptionManager";
import type { ViewSubscriptionManager } from "./viewSubscriptionManager";
import type { ViewSubscriptionDefinition } from "./viewSubscriptionProtocol";

const TENANT_A = "tenant-a";
const TENANT_B = "tenant-b";
const HEALTH_KEY = ["view", "health"] as const;

function healthView(): ViewSubscriptionDefinition {
  return {
    viewId: "health",
    query: { kind: "service-status" },
    timeRange: { mode: "rolling", window: "15m" },
    queryKey: HEALTH_KEY,
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

  async function start(tenantId = TENANT_A) {
    manager.update({ tenantId, views: [healthView()] });
    await settle();
  }

  beforeEach(() => {
    vi.useFakeTimers();
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
});
