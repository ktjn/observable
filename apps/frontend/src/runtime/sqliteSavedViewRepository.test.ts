import { describe, expect, it } from "vitest";
import type { CreateSavedViewRequest } from "../api/savedViews";
import { SqliteSavedViewRepository } from "./sqliteSavedViewRepository";

const TENANT_ID = "tenant-sqlite-test";
const OTHER_TENANT_ID = "tenant-sqlite-other";
const request: CreateSavedViewRequest = {
  name: "Checkout errors",
  signal_kind: "logs",
  config: {
    query: "checkout",
    severity_filter: "ERROR",
    time_range: { mode: "preset", preset: "1h" },
    visible_columns: ["timestamp", "body"],
  },
};

describe("SqliteSavedViewRepository", () => {
  it("persists CRUD and grants in SQLite", async () => {
    const repository = await SqliteSavedViewRepository.open();

    const created = repository.create(TENANT_ID, request);
    expect(repository.list(TENANT_ID, "logs").items).toEqual([created]);
    expect(repository.listGrants(TENANT_ID, created.saved_view_id).grants).toHaveLength(1);

    const updated = repository.update(TENANT_ID, created.saved_view_id, {
      ...request,
      name: "Updated checkout errors",
      visibility: "public",
    });
    expect(updated.name).toBe("Updated checkout errors");
    expect(updated.visibility).toBe("public");

    repository.addGrant(TENANT_ID, created.saved_view_id, "reviewer", "viewer");
    expect(repository.listGrants(TENANT_ID, created.saved_view_id).grants).toHaveLength(2);
    repository.revokeGrant(TENANT_ID, created.saved_view_id, "reviewer");
    expect(repository.listGrants(TENANT_ID, created.saved_view_id).grants).toHaveLength(1);

    repository.delete(TENANT_ID, created.saved_view_id);
    expect(repository.list(TENANT_ID, "logs").items).toEqual([]);
  });

  it("does not cross tenant boundaries", async () => {
    const repository = await SqliteSavedViewRepository.open();
    const created = repository.create(TENANT_ID, request);

    expect(repository.list(OTHER_TENANT_ID, "logs").items).toEqual([]);
    expect(repository.listGrants(OTHER_TENANT_ID, created.saved_view_id).grants).toEqual([]);
    expect(() => repository.update(OTHER_TENANT_ID, created.saved_view_id, request)).toThrow("404");
  });
});
