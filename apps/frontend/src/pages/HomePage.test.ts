import { reviewStatus } from "./HomePage";
import type { ServiceReview } from "../runtime/catalogContract";

function review(status: string | null, live = true): ServiceReview {
  return {
    service: { id: crypto.randomUUID(), business_name: "Checkout", criticality: "critical", lifecycle: live ? "live" : "retired" },
    environment_class: "production",
    live,
    health: live ? { state: "unknown", evidence_status: "missing", evaluated_at: null, last_evidence_at: null } : null,
    expectation: live && status ? { status: status as "compliant", reason: "current", objectives: [] } : null,
  };
}

describe("reviewStatus", () => {
  it("reports the worst expectation and never within expectations while one is unknown", () => {
    expect(reviewStatus([review("compliant")], false).label).toBe("Within expectations");
    expect(reviewStatus([review("compliant"), review("unknown")], false).label).toBe("Expectations unknown");
    expect(reviewStatus([review("unknown"), review("at_risk")], false).label).toBe("Expectations at risk");
    expect(reviewStatus([review("at_risk"), review("breached")], false).label).toBe("Expectations breached");
  });

  it("ignores services that are not live and discloses an unavailable review", () => {
    expect(reviewStatus([review(null, false)], false).label).toBe("No business-critical services");
    expect(reviewStatus(undefined, true).label).toBe("Expectations unavailable");
    expect(reviewStatus([review("compliant"), review(null, false)], false).detail)
      .toContain("across 1 business-critical production services");
  });
});
