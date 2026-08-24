import { describe, expect, it } from "vitest";
import { SqliteSloRepository } from "./sqliteSloRepository";

describe("SqliteSloRepository", () => {
  it("seeds and creates tenant-scoped SLO definitions", async () => {
    const repository = await SqliteSloRepository.open();
    const tenant = "00000000-0000-0000-0000-000000000001";
    expect(repository.list(tenant).items).toHaveLength(1);
    const created = repository.create(tenant, { service_name: "payments", environment: "production", target: 99.95, window_days: 30, burn_rate_fast_threshold: 14.4, burn_rate_slow_threshold: 6, description: "payments availability" });
    expect(repository.list(tenant).items).toContainEqual(created);
    expect(repository.list("other-tenant").items).toEqual([]);
  });
});
