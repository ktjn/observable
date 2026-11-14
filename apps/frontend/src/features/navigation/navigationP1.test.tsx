import { describe, expect, test, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { TimeDisplayProvider } from "../../lib/timeDisplay";
import {
  clearInvestigationTrail,
  pushInvestigationStep,
  readInvestigationTrail,
  recordRecentEntry,
  readRecentEntries,
} from "../../components/ScopeBar";
import { DataFreshness } from "../../components/ui/data-freshness";
import { buildAlertScopedTarget } from "../alerts/AlertRuleDetailPage";

// Mock TanStack Router primitives so components render cleanly in unit tests
let mockPathname = "/services/checkout";
let mockSearchStr = "?env=prod&preset=15m&status=error";

vi.mock("@tanstack/react-router", () => ({
  useLocation: () => ({ pathname: mockPathname, searchStr: mockSearchStr }),
  useParams: () => ({ serviceId: "checkout", dashboardId: "dash-1", ruleId: "rule-1" }),
  useRouter: () => ({ navigate: vi.fn() }),
  Link: ({ children, to, ...rest }: { children: ReactNode; to?: string; [k: string]: unknown }) => (
    <a href={typeof to === "string" ? to : "#"} {...rest}>
      {children}
    </a>
  ),
}));

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({
    tenantId: "t-1",
    tenantName: "Acme Corp",
    environment: "prod",
    setTenant: vi.fn(),
    setEnvironment: vi.fn(),
  }),
}));

vi.mock("../../hooks/useGlobalDateRange", () => ({
  PRESET_OPTIONS: [{ value: "15m", label: "Last 15m" }, { value: "1h", label: "Last 1h" }],
  presetToMs: () => 3600_000,
  useGlobalDateRange: () => ({ preset: "15m", fromMs: 1000, toMs: 2000 }),
}));

const mockDashboardCreate = vi.fn(async (_tenant: string, req: { name: string }) => ({
  dashboard_id: "dash-team-copy",
  name: req.name,
  panels: [],
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
}));

const mockRuntime = {
  services: {
    list: async () => ({
      items: [
        {
          service_name: "checkout",
          request_rate: 120,
          error_rate: 0.08,
          p95_latency_ms: 620,
          active_alert_count: 2,
          slo_breaching: true,
        },
      ],
    }),
    summary: async () => ({
      service: {
        service_name: "checkout",
        request_rate: 120,
        error_rate: 0.08,
        p95_latency_ms: 620,
        active_alert_count: 2,
        slo_breaching: true,
      },
    }),
    responseTimeHistory: async () => ({ buckets: [] }),
  },
  alerts: {
    list: async () => ({
      items: [
        {
          rule_id: "rule-1",
          name: "Checkout High Latency",
          severity: "critical",
          firing: true,
          silenced: false,
          alert_type: "threshold",
          condition: { metric_name: "p95", operator: "gt", threshold: 500 },
        },
      ],
    }),
  },
  incidents: {
    list: async () => ({
      items: [
        {
          incident_id: "inc-42",
          title: "checkout latency spike",
          severity: "critical",
          status: "open",
          service_name: "checkout",
        },
      ],
    }),
  },
  savedViews: {
    list: async () => ({
      items: [
        {
          saved_view_id: "sv-1",
          name: "Checkout 5xx Investigation",
          signal_kind: "traces",
          visibility: "public",
          config: { query: "status=error", service: "checkout", time_range: { mode: "preset", preset: "15m" } },
          created_at: new Date().toISOString(),
        },
      ],
    }),
  },
  dashboards: {
    get: async () => ({
      dashboard_id: "dash-1",
      name: "Checkout Overview",
      owner: "Checkout SRE",
      description: "Service health and error budget tracking",
      updated_at: new Date().toISOString(),
      panels: [],
    }),
    create: mockDashboardCreate,
    update: vi.fn(),
  },
  catalog: {
    reviews: async () => ({
      confidence: { confidence: "high" },
      items: [
        {
          service: {
            id: "svc-1",
            business_name: "Checkout API",
            domain: "Order Checkout",
            accountable: true,
          },
          health: { state: "degraded" },
          expectation: { status: "breached" },
          issues: { open: 1 },
          confidence: { confidence: "high" },
        },
      ],
    }),
  },
};

vi.mock("../../hooks/useRuntime", () => ({
  useRuntime: () => mockRuntime,
}));

vi.mock("../../features/catalog/catalogUi", () => ({
  useCatalogQuery: () => ({
    isPending: false,
    isError: false,
    data: {
      confidence: { confidence: "high" },
      items: [
        {
          service: {
            id: "svc-1",
            business_name: "Checkout API",
            domain: "Order Checkout",
            accountable: true,
          },
          health: { state: "degraded" },
          expectation: { status: "breached" },
          issues: { open: 1 },
          confidence: { confidence: "high" },
        },
      ],
    },
  }),
}));

vi.mock("../../api/reliability", () => ({
  getServiceReliabilityReport: async () => ({
    slos: [{ sli_type: "latency", target_latency_ms: 400 }],
  }),
}));

import { ScopeBar } from "../../components/ScopeBar";
import HomePage from "../../pages/HomePage";
import ServiceDetailPage from "../../pages/ServiceDetailPage";
import DashboardDetailPage from "../../pages/DashboardDetailPage";

function wrap(node: ReactNode) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <TimeDisplayProvider>{node}</TimeDisplayProvider>
    </QueryClientProvider>
  );
}

describe("Navigation and Role-Based Workspaces P1", () => {
  beforeEach(() => {
    clearInvestigationTrail();
    localStorage.clear();
    mockDashboardCreate.mockClear();
  });

  test("obs_20_05_backward_navigation_preserves_scope", () => {
    pushInvestigationStep({
      pathAndSearch: "/services/checkout?env=prod&preset=15m&status=error",
      label: "Service: checkout",
      service: "checkout",
      environment: "prod",
      preset: "Last 15m",
    });
    mockPathname = "/traces";
    mockSearchStr = "?service=checkout&env=prod&preset=15m";

    render(wrap(<ScopeBar />));
    const backLink = screen.getByTestId("investigation-back-link");
    expect(backLink).toHaveAttribute(
      "href",
      "#/services/checkout?env=prod&preset=15m&status=error",
    );
  });

  test("obs_20_09_identifies_location_and_investigation_path", () => {
    pushInvestigationStep({
      pathAndSearch: "/",
      label: "Overview",
    });
    pushInvestigationStep({
      pathAndSearch: "/services/checkout?preset=15m",
      label: "Service: checkout",
    });
    mockPathname = "/incidents/inc-42";
    mockSearchStr = "?preset=15m";

    render(wrap(<ScopeBar />));
    const trail = screen.getByTestId("investigation-trail");
    expect(trail).toHaveTextContent("Overview");
    expect(trail).toHaveTextContent("Service: checkout");
    expect(trail).toHaveTextContent("Incident: inc-42");
    expect(readInvestigationTrail().length).toBe(3);
  });

  test("obs_20_06_saved_and_recent_entries_reopen_without_reconstructing", async () => {
    recordRecentEntry({
      pathAndSearch: "/services/checkout?preset=15m",
      label: "Service: checkout",
      kind: "service",
    });
    expect(readRecentEntries().length).toBe(1);

    render(wrap(<HomePage />));
    const panel = await screen.findByTestId("saved-recent-panel");
    const savedLink = await screen.findByRole("link", { name: "Checkout 5xx Investigation" });
    expect(savedLink).toHaveAttribute("href", "#/traces?q=status%3Derror&service=checkout&preset=15m");
    expect(panel).toHaveTextContent("Service: checkout");
  });

  test("obs_20_07_and_21_02_service_view_exposes_related_incidents_and_commitments", async () => {
    mockPathname = "/services/checkout";
    mockSearchStr = "";
    render(wrap(<ServiceDetailPage />));
    const related = await screen.findByTestId("related-context-panel");
    await waitFor(() => {
      expect(related).toHaveTextContent("Incident: checkout latency spike");
      expect(related).toHaveTextContent("SLOs (1) — Breaching");
    });
    expect(related).toHaveTextContent("Recent changes");
    expect(related).toHaveTextContent("Dependencies & topology");
  });

  test("obs_20_10_alert_deep_links_with_service_env_and_window_preselected", () => {
    const target = buildAlertScopedTarget({
      service_name: "checkout",
      environment: "prod",
      incident_id: "inc-42",
      condition: { window_secs: 900 },
      firing_items: [{ fired_at: "2026-11-14T12:00:00Z" } as never],
    });
    expect(target.kind).toBe("incident");
    expect(target.href).toContain("/incidents/inc-42?");
    expect(target.href).toContain("service=checkout");
    expect(target.href).toContain("env=prod");
    expect(target.href).toContain("from=");
    expect(target.href).toContain("to=");
  });

  test("obs_21_04_and_21_09_role_prioritization_and_business_journeys", async () => {
    render(wrap(<HomePage />));
    const critical = await screen.findByTestId("accountable-critical-issues");
    await waitFor(() => {
      expect(critical).toHaveTextContent("checkout latency spike — critical incident");
    });

    const switcher = screen.getByTestId("workspace-role-switcher");
    fireEvent.change(switcher, { target: { value: "business_stakeholder" } });

    const journeys = await screen.findByTestId("business-journeys-panel");
    expect(journeys).toHaveTextContent("Order Checkout");
    expect(journeys).toHaveTextContent("Degraded");
    expect(journeys).toHaveTextContent("Checkout API");
  });

  test("obs_21_05_and_21_06_dashboard_team_copy_and_governance_metadata", async () => {
    render(wrap(<DashboardDetailPage />));
    const gov = await screen.findByTestId("dashboard-governance-meta");
    expect(gov).toHaveTextContent("Owner: Checkout SRE");
    expect(gov).toHaveTextContent("Purpose: Service health and error budget tracking");
    expect(gov).toHaveTextContent("Maintained");

    const dupBtn = screen.getByTestId("duplicate-dashboard-button");
    fireEvent.click(dupBtn);
    await waitFor(() => {
      expect(mockDashboardCreate).toHaveBeenCalledWith(
        "t-1",
        expect.objectContaining({ name: "Checkout Overview (Team view)" }),
      );
    });
  });

  test("obs_21_07_distinguishes_live_historical_and_scheduled_reports", () => {
    const { rerender } = render(<DataFreshness dataUpdatedAt={Date.now()} viewMode="live" />);
    expect(screen.getByTestId("view-freshness-mode")).toHaveTextContent("Live operational");

    rerender(<DataFreshness dataUpdatedAt={Date.now()} viewMode="historical" />);
    expect(screen.getByTestId("view-freshness-mode")).toHaveTextContent("Historical analysis");

    rerender(
      <DataFreshness
        dataUpdatedAt={Date.now()}
        viewMode="scheduled_report"
        scheduleLabel="Daily 08:00 UTC"
      />,
    );
    expect(screen.getByTestId("view-freshness-mode")).toHaveTextContent(
      "Scheduled report (Daily 08:00 UTC)",
    );
  });
});
