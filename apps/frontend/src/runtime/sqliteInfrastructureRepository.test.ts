import { describe, expect, it } from "vitest";
import { SqliteInfrastructureRepository } from "./sqliteInfrastructureRepository";

describe("SqliteInfrastructureRepository", () => {
  it("lists and retrieves tenant-scoped inventory entities", async () => {
    const repository = await SqliteInfrastructureRepository.open();
    const tenantId = "00000000-0000-0000-0000-000000000001";

    expect(repository.list(tenantId, "pod")).toHaveLength(2);
    expect(repository.get(tenantId, "container", "playground-container-payment")).toMatchObject({
      display_name: "payment",
      health_state: "watch",
    });
    expect(repository.list("other-tenant")).toEqual([]);
  });
});
