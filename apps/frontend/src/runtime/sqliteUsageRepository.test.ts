import { describe, expect, it } from "vitest";
import { SqliteUsageRepository } from "./sqliteUsageRepository";

describe("SqliteUsageRepository", () => {
  it("returns a tenant-scoped usage report", async () => {
    const repository = await SqliteUsageRepository.open();
    const report = repository.report("00000000-0000-0000-0000-000000000001", { from: 0, to: 1000 });
    expect(report.telemetry_summary.spans).toBe(240);
    expect(report.from).toBe("1970-01-01T00:00:00.000Z");
    expect(() => repository.report("other-tenant", {})).toThrow("404");
  });
});
