import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useTenantSwitch } from "./useTenantSwitch";
import { createPlaygroundViewSubscriptions } from "../runtime/playgroundViewSubscriptions";
import { createViewSubscriptionManager } from "../runtime/viewSubscriptionManager";
import type { ViewSubscriptionDefinition } from "../runtime/viewSubscriptionProtocol";

const setTenant = vi.fn();
vi.mock("./useTenantContext", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./useTenantContext")>();
  return { ...actual, useTenantContext: () => ({ setTenant }) };
});

const switchTenant = vi.fn(async (_tenantId: string) => ({
  tenant_id: "tenant-b",
  role: "admin",
}));
vi.mock("../api/auth", () => ({ switchTenant: (id: string) => switchTenant(id) }));

const TENANT_A = "tenant-a";
const HEALTH_KEY = ["view", "health"] as const;

const healthView: ViewSubscriptionDefinition = {
  viewId: "health",
  query: { kind: "service-status" },
  timeRange: { mode: "rolling", window: "15m" },
  queryKey: HEALTH_KEY,
};

describe("useTenantSwitch", () => {
  let queryClient: QueryClient;

  function wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  }

  beforeEach(() => {
    queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    vi.clearAllMocks();
  });

  afterEach(() => {
    queryClient.clear();
  });

  it("authorizes the switch on the server before adopting the new tenant locally", async () => {
    const { result } = renderHook(() => useTenantSwitch(), { wrapper });
    await result.current({ id: "tenant-b", name: "other" });
    expect(switchTenant).toHaveBeenCalledWith("tenant-b");
    expect(setTenant).toHaveBeenCalledWith({ id: "tenant-b", name: "other" });
  });

  it("clears the previous tenant's query cache", async () => {
    queryClient.setQueryData(HEALTH_KEY, { payments: { health: "normal" } });
    const { result } = renderHook(() => useTenantSwitch(), { wrapper });
    await result.current({ id: "tenant-b", name: "other" });
    expect(queryClient.getQueryData(HEALTH_KEY)).toBeUndefined();
  });

  it("closes every open read-only view stream", async () => {
    const fake = createPlaygroundViewSubscriptions({ sessionTenantId: TENANT_A });
    fake.control.setSnapshotData("health", { payments: { health: "normal" } });
    const manager = createViewSubscriptionManager({
      queryClient,
      transport: fake.transport,
      confirmTenant: fake.confirmTenant,
    });
    manager.update({ tenantId: TENANT_A, views: [healthView] });
    await Promise.resolve();
    await Promise.resolve();
    expect(fake.control.isConnected()).toBe(true);

    const { result } = renderHook(() => useTenantSwitch(), { wrapper });
    await result.current({ id: "tenant-b", name: "other" });

    expect(fake.control.isConnected()).toBe(false);
    expect(manager.getState().status).toBe("idle");
    manager.dispose();
  });

  it("does not adopt the new tenant when the server refuses the switch", async () => {
    switchTenant.mockRejectedValueOnce(new Error("switchTenant failed: 403"));
    const { result } = renderHook(() => useTenantSwitch(), { wrapper });
    await expect(result.current({ id: "tenant-b", name: "other" })).rejects.toThrow("403");
    expect(setTenant).not.toHaveBeenCalled();
  });
});
