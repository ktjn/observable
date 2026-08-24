import { describe, expect, it } from "vitest";
import { SqliteAlertRuleRepository } from "./sqliteAlertRuleRepository";

const TENANT_ID = "00000000-0000-0000-0000-000000000001";
const OTHER_TENANT_ID = "tenant-alert-other";

describe("SqliteAlertRuleRepository", () => {
  it("seeds and persists alert rule CRUD state", async () => {
    const repository = await SqliteAlertRuleRepository.open();
    expect(repository.list(TENANT_ID).items).toHaveLength(3);

    const ruleId = repository.create(TENANT_ID, {
      name: "checkout errors",
      metric_name: "http.server.errors.rate",
      operator: "gt",
      threshold: 5,
      runbook_url: "https://runbooks.example/checkout",
    });
    expect(repository.get(TENANT_ID, ruleId).name).toBe("checkout errors");
    expect(repository.getRunbook(TENANT_ID, ruleId)).toBe("https://runbooks.example/checkout");

    repository.setSilenced(TENANT_ID, ruleId, true);
    expect(repository.get(TENANT_ID, ruleId).state).toBe("silenced");
    repository.setRunbook(TENANT_ID, ruleId, null);
    expect(repository.getRunbook(TENANT_ID, ruleId)).toBeNull();
  });

  it("enforces tenant isolation for reads and writes", async () => {
    const repository = await SqliteAlertRuleRepository.open();
    expect(repository.list(OTHER_TENANT_ID).items).toEqual([]);
    expect(() => repository.get(OTHER_TENANT_ID, "playground-rule-1")).toThrow("404");
    expect(() => repository.setSilenced(OTHER_TENANT_ID, "playground-rule-1", true)).toThrow("404");
  });
});
