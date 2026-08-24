import { describe, expect, it } from "vitest";
import type { CreateDashboardRequest } from "../api/dashboards";
import { SqliteDashboardRepository } from "./sqliteDashboardRepository";

const TENANT_ID = "tenant-dashboard-test";
const OTHER_TENANT_ID = "tenant-dashboard-other";
const request: CreateDashboardRequest = {
  name: "Checkout overview",
  panels: [
    {
      title: "Checkout errors",
      query_kind: "logs",
      preset: "1h",
      filters: { severity: "ERROR" },
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "global" },
    },
  ],
};

describe("SqliteDashboardRepository", () => {
  it("persists tenant-scoped dashboard CRUD and panel JSON", async () => {
    const repository = await SqliteDashboardRepository.open();

    const created = repository.create(TENANT_ID, request);
    expect(repository.list(TENANT_ID).items).toEqual([created]);
    expect(created.panels[0]).toMatchObject({ title: "Checkout errors", panel_kind: "query" });

    const updated = repository.update(TENANT_ID, created.dashboard_id, {
      name: "Updated checkout overview",
      panels: [
        {
          ...request.panels[0],
          panel_id: created.panels[0].panel_id,
          panel_kind: "text",
          query_kind: null,
          content: "Checkout error context",
          layout: { x: 1, y: 2, w: 8, h: 5 },
          time_range: { mode: "preset", preset: "3h" },
        },
      ],
    });
    expect(repository.get(TENANT_ID, created.dashboard_id)).toEqual(updated);
    expect(updated.panels[0]).toMatchObject({ panel_kind: "text", content: "Checkout error context" });

    repository.delete(TENANT_ID, created.dashboard_id);
    expect(repository.list(TENANT_ID).items).toEqual([]);
    expect(() => repository.get(TENANT_ID, created.dashboard_id)).toThrow("Dashboard not found");
  });

  it("does not cross tenant boundaries", async () => {
    const repository = await SqliteDashboardRepository.open();
    const created = repository.create(TENANT_ID, request);

    expect(repository.list(OTHER_TENANT_ID).items).toEqual([]);
    expect(() => repository.get(OTHER_TENANT_ID, created.dashboard_id)).toThrow("Dashboard not found");
    expect(() => repository.update(OTHER_TENANT_ID, created.dashboard_id, {
      name: "Wrong tenant",
      panels: [],
    })).toThrow("Dashboard not found");
  });
});
