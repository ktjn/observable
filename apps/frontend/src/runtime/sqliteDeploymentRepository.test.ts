import { describe, expect, it } from "vitest";
import { SqliteDeploymentRepository } from "./sqliteDeploymentRepository";

describe("SqliteDeploymentRepository", () => {
  it("filters deployment markers by tenant and service", async () => {
    const repository = await SqliteDeploymentRepository.open();
    expect(repository.list("00000000-0000-0000-0000-000000000001", { service_name: "payment" })).toHaveLength(2);
    expect(repository.list("other-tenant")).toEqual([]);
  });
});
