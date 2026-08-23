import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, it, expect, beforeEach } from "vitest";
import { ServiceAlertsTab } from "./ServiceAlertsTab";
import type { AlertRuleItem } from "../../api/alerts";
import type { IncidentItem } from "../../api/incidents";
import type { RuntimeApi } from "../../runtime/types";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & {
      children?: React.ReactNode;
    }) => <a {...props}>{children}</a>,
  };
});

import { useRuntime } from "../../hooks/useRuntime";

const firingRule: AlertRuleItem = {
  rule_id: "rule-1",
  name: "High CPU",
  metric_name: "cpu_usage",
  operator: "gt",
  threshold: 90,
  severity: "critical",
  silenced: false,
  state: "active",
  firing: true,
  last_fired_at: "2026-05-15T10:00:00Z",
  notification_channels: [],
  auto_trigger_incident: false,
  suppressed: false,
};

const okRule: AlertRuleItem = {
  rule_id: "rule-2",
  name: "Low Memory",
  metric_name: "memory_free",
  operator: "lt",
  threshold: 10,
  severity: "warning",
  silenced: false,
  state: "ok",
  firing: false,
  notification_channels: [],
  auto_trigger_incident: false,
  suppressed: false,
};

const openIncident: IncidentItem = {
  incident_id: "inc-1",
  title: "Database overload",
  severity: "critical",
  status: "triggered",
  triggered_at: "2026-05-15T09:00:00Z",
  triggered_by_rule_id: "rule-1",
};

function mockRuntime(alerts: AlertRuleItem[], incidents: IncidentItem[]) {
  vi.mocked(useRuntime).mockReturnValue({
    alerts: { list: vi.fn(async () => ({ items: alerts })) },
    incidents: { list: vi.fn(async () => ({ items: incidents })) },
  } as unknown as RuntimeApi);
}

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("ServiceAlertsTab", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows section headings", async () => {
    mockRuntime([], []);

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("Firing Alert Rules"));
    expect(screen.getByText("Open Incidents")).toBeInTheDocument();
  });

  it("shows only firing rules, not OK rules", async () => {
    mockRuntime([firingRule, okRule], []);

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("High CPU"));
    expect(screen.queryByText("Low Memory")).not.toBeInTheDocument();
  });

  it("shows open incidents with link to detail", async () => {
    mockRuntime([], [openIncident]);

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("Database overload"));
    expect(screen.getByText("Database overload")).toBeInTheDocument();
  });

  it("shows no-firing and no-incidents placeholders when empty", async () => {
    mockRuntime([okRule], []);

    render(<ServiceAlertsTab />, { wrapper });

    await waitFor(() => screen.getByText("No firing rules."));
    expect(screen.getByText("No open incidents.")).toBeInTheDocument();
  });
});
