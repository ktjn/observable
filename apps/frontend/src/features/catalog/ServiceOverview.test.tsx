import { render, screen } from "@testing-library/react";
import type { OverviewItem } from "../../runtime/catalogContract";
import { changedServices, EvidenceAge, IssueCount, sortItems } from "./ServiceOverview";

function item(name: string, state: OverviewItem["state"], ageSeconds: number | null): OverviewItem {
  return {
    service: { id: `id-${name}`, business_name: name, criticality: "critical", lifecycle: "live" },
    live: true,
    state,
    freshness: { status: ageSeconds == null ? "missing" : "current", last_evidence_at: null, age_seconds: ageSeconds, bound_seconds: 300 },
    issues: { status: "current", open: 0 },
    fit_for_decision: state === "normal",
    unfit_reasons: [],
  };
}

const items = [item("Billing", "normal", 10), item("Checkout", "degraded", 20), item("Search", "unknown", null), item("Auth", "unavailable", 5)];

describe("sortItems", () => {
  it("puts the worst state first by default", () => {
    expect(sortItems(items, "state").map((i) => i.service.business_name)).toEqual(["Auth", "Checkout", "Search", "Billing"]);
  });

  it("sorts missing evidence as oldest, never as fresh", () => {
    expect(sortItems(items, "age").map((i) => i.service.business_name)).toEqual(["Search", "Checkout", "Billing", "Auth"]);
  });

  it("sorts by name", () => {
    expect(sortItems(items, "name").map((i) => i.service.business_name)).toEqual(["Auth", "Billing", "Checkout", "Search"]);
  });
});

test("changedServices names services whose state changed", () => {
  const after = [item("Billing", "degraded", 10), item("Checkout", "degraded", 20), item("New", "normal", 1)];
  expect(changedServices(items, after)).toEqual(["Billing"]);
});

describe("IssueCount", () => {
  it("shows a count only for a list that was read", () => {
    render(<IssueCount issues={{ status: "current", open: 2 }} />);
    expect(screen.getByText("2 open")).toBeInTheDocument();
  });

  it.each([
    [{ status: "evaluation_unavailable", open: null }, "Unavailable"],
    [{ status: "restricted", open: null }, "Restricted"],
    [null, "Unavailable"],
  ])("never shows an unread list (%o) as zero", (issues, label) => {
    const { container } = render(<IssueCount issues={issues} />);
    expect(container).toHaveTextContent(label);
    expect(container.textContent).not.toMatch(/0|None/);
  });
});

test("EvidenceAge marks stale and missing evidence", () => {
  const { container } = render(<>
    <EvidenceAge freshness={{ status: "stale", last_evidence_at: null, age_seconds: 1200, bound_seconds: 300 }} />
    <EvidenceAge freshness={null} />
  </>);
  expect(container).toHaveTextContent("Stale · last 20 min ago");
  expect(container).toHaveTextContent("Unavailable");
});
