import { describe, it, expect, vi, beforeEach } from "vitest";

describe("auth api", () => {
  beforeEach(() => {
    vi.stubGlobal("fetch", vi.fn());
  });

  it("me() returns user when authenticated", async () => {
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      ok: true,
      json: async () => ({
        user_id: "abc",
        email: "alice@example.com",
        tenants: [{ tenant_id: "t1", role: "member" }],
      }),
    });

    const { me } = await import("./auth");
    const user = await me();
    expect(user.email).toBe("alice@example.com");
    expect(user.tenants).toHaveLength(1);
  });

  it("me() throws when unauthenticated", async () => {
    (fetch as ReturnType<typeof vi.fn>).mockResolvedValueOnce({ ok: false, status: 401 });

    const { me } = await import("./auth");
    await expect(me()).rejects.toThrow("401");
  });
});
