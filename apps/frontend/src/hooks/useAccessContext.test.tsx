import { renderHook, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { useAccessContext } from "./useAccessContext";

let mockUser: {
  user_id: string;
  email: string;
  tenants: { tenant_id: string; role: string }[];
  elevated_access?: { active: boolean; scope?: string; expires_at?: string } | null;
} | null = null;

vi.mock("./useAuth", () => ({
  useAuth: () => ({ data: mockUser, isLoading: false }),
}));

vi.mock("./useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "t-1", tenantName: "acme" }),
}));

describe("useAccessContext", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-14T08:00:00Z"));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("denies incident mutation for viewer role and allows for admin", () => {
    mockUser = {
      user_id: "u-1",
      email: "viewer@acme.test",
      tenants: [{ tenant_id: "t-1", role: "viewer" }],
    };
    const { result, rerender } = renderHook(() => useAccessContext());
    expect(result.current.canMutateIncidents).toBe(false);
    expect(result.current.isElevated).toBe(false);

    mockUser = {
      user_id: "u-2",
      email: "admin@acme.test",
      tenants: [{ tenant_id: "t-1", role: "admin" }],
    };
    rerender();
    expect(result.current.canMutateIncidents).toBe(true);
  });

  it("marks elevated access active and revokes it automatically upon expiry", () => {
    mockUser = {
      user_id: "u-2",
      email: "admin@acme.test",
      tenants: [{ tenant_id: "t-1", role: "admin" }],
      elevated_access: {
        active: true,
        scope: "incident:write",
        expires_at: "2026-11-14T08:00:05Z",
      },
    };
    const { result } = renderHook(() => useAccessContext());
    expect(result.current.isElevated).toBe(true);

    act(() => {
      vi.advanceTimersByTime(5100);
    });
    expect(result.current.isElevated).toBe(false);
    expect(result.current.elevation).toBeNull();
  });
});
