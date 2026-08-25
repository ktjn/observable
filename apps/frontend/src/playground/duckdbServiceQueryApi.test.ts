import { describe, expect, it, vi } from "vitest";
import { DuckDbServiceQueryApi } from "./duckdbServiceQueryApi";

describe("DuckDbServiceQueryApi", () => {
  it("executes planned service queries and lists service names", async () => {
    const query = vi.fn().mockResolvedValue({ toArray: () => [{ service_name: "web" }] });
    const api = new DuckDbServiceQueryApi({ query });

    await expect(api.executePlanned("SELECT service_name FROM spans")).resolves.toEqual([
      { service_name: "web" },
    ]);
    await expect(api.listNames()).resolves.toEqual([{ service_name: "web" }]);
    expect(query).toHaveBeenNthCalledWith(2, expect.stringContaining("DISTINCT service_name"));
  });
});
