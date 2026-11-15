import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, renderHook, screen } from "@testing-library/react";
import { useState } from "react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useReadonlyViewSubscription } from "./useReadonlyViewSubscription";
import { createPlaygroundViewSubscriptions } from "../runtime/playgroundViewSubscriptions";
import type { PlaygroundViewSubscriptions } from "../runtime/playgroundViewSubscriptions";
import { LS_TENANT_ID } from "./useTenantContext";
import type { RuntimeApi } from "../runtime/types";
import type { ViewSubscriptionDefinition } from "../runtime/viewSubscriptionProtocol";

vi.mock("./useRuntime", () => ({ useRuntime: vi.fn() }));
import { useRuntime } from "./useRuntime";

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

describe("useReadonlyViewSubscription", () => {
  let queryClient: QueryClient;
  let fake: PlaygroundViewSubscriptions;

  function wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  }

  async function flush() {
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
  }

  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    fake = createPlaygroundViewSubscriptions({ sessionTenantId: TENANT_A });
    fake.control.setSnapshotData("health", { payments: { health: "normal" } });
    vi.mocked(useRuntime).mockReturnValue({
      viewSubscriptions: { transport: fake.transport, confirmTenant: fake.confirmTenant },
    } as unknown as RuntimeApi);
  });

  afterEach(() => {
    queryClient.clear();
    localStorage.clear();
    vi.clearAllMocks();
  });

  it("starts idle and reports live once the snapshot stream is connected", async () => {
    const { result } = renderHook(
      () => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }),
      { wrapper },
    );
    await flush();
    expect(result.current.status).toBe("live");
    expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });
  });

  it("does not subscribe when disabled", async () => {
    renderHook(
      () =>
        useReadonlyViewSubscription({
          tenantId: TENANT_A,
          views: [healthView()],
          enabled: false,
        }),
      { wrapper },
    );
    await flush();
    expect(fake.control.openCount()).toBe(0);
  });

  it("reports paused for a pinned historical view and never applies pushed data", async () => {
    const { result } = renderHook(
      () =>
        useReadonlyViewSubscription({
          tenantId: TENANT_A,
          views: [{ ...healthView(), timeRange: { mode: "fixed", from: 1, to: 2 } }],
        }),
      { wrapper },
    );
    await flush();
    expect(result.current.status).toBe("paused");
    expect(fake.control.openCount()).toBe(0);
    expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
  });

  it("does not reopen the stream when re-rendered with an equal view set", async () => {
    const { rerender } = renderHook(
      () => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }),
      { wrapper },
    );
    await flush();
    rerender();
    rerender();
    await flush();
    expect(fake.control.openCount()).toBe(1);
  });

  it("applies a live delta to the cache the view already reads from", async () => {
    renderHook(() => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }), {
      wrapper,
    });
    await flush();
    await act(async () => {
      fake.control.delta("health", [
        { op: "upsert", key: "payments", value: { health: "degraded" } },
      ]);
      await Promise.resolve();
    });
    expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "degraded" } });
  });

  it("preserves consumer component state across a reconnect", async () => {
    let mountCount = 0;

    function Consumer() {
      const { status } = useReadonlyViewSubscription({
        tenantId: TENANT_A,
        views: [healthView()],
      });
      const [selectedRow] = useState(() => {
        mountCount += 1;
        return "payments";
      });
      return (
        <div>
          <span data-testid="status">{status}</span>
          <span data-testid="selected">{selectedRow}</span>
        </div>
      );
    }

    render(<Consumer />, { wrapper });
    await flush();
    expect(screen.getByTestId("status")).toHaveTextContent("live");

    await act(async () => {
      fake.control.setAutoConnect(false);
      fake.control.disconnect();
      await Promise.resolve();
    });
    expect(screen.getByTestId("status")).toHaveTextContent("reconnecting");
    expect(screen.getByTestId("selected")).toHaveTextContent("payments");
    expect(mountCount).toBe(1);
  });

  it("isolates the previous tenant's view data when the tenant changes", async () => {
    const { rerender } = renderHook(
      ({ tenantId }: { tenantId: string }) =>
        useReadonlyViewSubscription({ tenantId, views: [healthView()] }),
      { wrapper, initialProps: { tenantId: TENANT_A } },
    );
    await flush();
    expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });

    fake.control.switchTenant(TENANT_B);
    rerender({ tenantId: TENANT_B });
    expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
  });

  it("closes and isolates the stream when another tab switches the session tenant", async () => {
    const { result } = renderHook(
      () => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }),
      { wrapper },
    );
    await flush();
    expect(queryClient.getQueryData(HEALTH_KEY)).toEqual({ payments: { health: "normal" } });

    await act(async () => {
      window.dispatchEvent(
        new StorageEvent("storage", { key: LS_TENANT_ID, newValue: TENANT_B }),
      );
    });

    expect(result.current.status).toBe("idle");
    expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
    expect(fake.control.isConnected()).toBe(false);
  });

  it("ignores a cross-tab storage event for its own tenant", async () => {
    const { result } = renderHook(
      () => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }),
      { wrapper },
    );
    await flush();
    await act(async () => {
      window.dispatchEvent(
        new StorageEvent("storage", { key: LS_TENANT_ID, newValue: TENANT_A }),
      );
    });
    expect(result.current.status).toBe("live");
  });

  it("closes the stream when the consumer unmounts", async () => {
    const { unmount } = renderHook(
      () => useReadonlyViewSubscription({ tenantId: TENANT_A, views: [healthView()] }),
      { wrapper },
    );
    await flush();
    expect(fake.control.isConnected()).toBe(true);
    unmount();
    expect(fake.control.isConnected()).toBe(false);
  });
});
