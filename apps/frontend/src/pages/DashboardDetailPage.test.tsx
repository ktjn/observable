import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, test, vi } from "vitest";
import * as dashboardsApi from "../api/dashboards";
import { submitNlqQuery } from "../api/nlq";
import DashboardDetailPage from "./DashboardDetailPage";
import { TimeDisplayProvider } from "../lib/timeDisplay";

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({
    preset: "1h",
    fromMs: 1_700_000_000_000,
    toMs: 1_700_003_600_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  }),
  presetToMs: () => 3_600_000,
}));

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useParams: () => ({ dashboardId: "dash-1" }),
  };
});

vi.mock("../api/nlq", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/nlq")>();
  return {
    ...actual,
    submitNlqQuery: vi.fn(),
  };
});

vi.mock('react-grid-layout', () => {
  type LayoutItem = { i: string; x: number; y: number; w: number; h: number };
  const MockRGL = ({
    children,
    onLayoutChange,
  }: {
    children: import('react').ReactNode;
    onLayoutChange: (layout: LayoutItem[]) => void;
  }) => (
    <div data-testid="rgl">
      <button
        type="button"
        onClick={() =>
          onLayoutChange([
            { i: 'query-1', x: 3, y: 0, w: 6, h: 4 },
            { i: 'text-1', x: 6, y: 0, w: 6, h: 2 },
          ])
        }
      >
        Simulate layout change
      </button>
      {children}
    </div>
  );
  MockRGL.displayName = 'MockRGL';
  const useContainerWidth = () => ({ width: 1200, containerRef: { current: null }, mounted: true });
  return { GridLayout: MockRGL, useContainerWidth };
});

const queryFrame = {
  type: "frame" as const,
  frame: {
    frame_type: "table" as const,
    x_field: null,
    y_field: null,
    series_field: null,
    unit: null,
    suggested_visualization: "table",
    field_roles: [],
    data: [{ service_name: "checkout", value: 42 }],
    nlq_ir: {
      operation: "table" as const,
      signals: ["logs" as const],
      filters: [],
      group_by: [],
      time_range: { from: "1", to: "2" },
      metric: null,
      window: null,
      resolution: null,
      visualization_hint: null,
    },
    source_sql: "select 1",
    time_range: { from: "1", to: "2" },
    signal_types: ["logs" as const],
    sample_rate: null,
    approximation_statement: "Approximate operational data.",
  },
};

const dashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "Checkout Health",
  visibility: "private",
  created_at: "2026-05-10T00:00:00Z",
  panels: [
    {
      panel_id: "query-1",
      title: "Error logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      filters: {},
      query_text: "errors in checkout",
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "global" },
    },
    {
      panel_id: "text-1",
      title: "Incident notes",
      panel_kind: "text",
      filters: {},
      content: "Escalate after deploy verification.",
      layout: { x: 6, y: 0, w: 6, h: 2 },
      time_range: { mode: "global" },
    },
  ],
};

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <TimeDisplayProvider>
        <DashboardDetailPage />
      </TimeDisplayProvider>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.mocked(submitNlqQuery).mockResolvedValue(queryFrame);
});

test("renders query panels through NLQ and text panels as explanation boxes", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();

  expect(await screen.findByRole("heading", { name: "Checkout Health" })).toBeInTheDocument();
  expect(await screen.findByText("Escalate after deploy verification.")).toBeInTheDocument();
  expect(await screen.findByText("checkout")).toBeInTheDocument();
  expect(submitNlqQuery).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      question: "errors in checkout",
      mode: "execute",
      service_name: "checkout",
    }),
  );
});

test("local preset time range overrides the global date range", async () => {
  vi.spyOn(Date, "now").mockReturnValue(new Date("2026-05-10T12:00:00Z").getTime());
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      {
        ...dashboard.panels[0],
        time_range: { mode: "preset", preset: "1h" },
      },
    ],
  });
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });

  await waitFor(() =>
    expect(submitNlqQuery).toHaveBeenCalledWith(
      "test-tenant",
      expect.objectContaining({
        base_ir: expect.objectContaining({
          time_range: {
            from: "1778410800000000000",
            to: "1778414400000000000",
          },
        }),
      }),
    ),
  );
});

test("add panel button opens template library and custom form submits a new query panel", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  await screen.findByRole("button", { name: "Add panel" });
  fireEvent.click(screen.getByRole("button", { name: "Add panel" }));

  expect(await screen.findByTestId("template-custom")).toBeInTheDocument();
  fireEvent.click(screen.getByTestId("template-custom"));

  expect(await screen.findByPlaceholderText("Panel title")).toBeInTheDocument();

  fireEvent.change(screen.getByPlaceholderText("Panel title"), { target: { value: "New metric" } });

  const submitButton = screen.getAllByRole("button", { name: "Add panel" })[1];
  fireEvent.click(submitButton);

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            title: "New metric",
            panel_kind: "query",
            query_kind: "logs",
            layout: expect.objectContaining({ x: 0, y: 4, w: 12, h: 4 }),
          }),
        ]),
      }),
    ),
  );
});

test("add panel form shows text content field when kind is text", async () => {
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Add panel" }));
  fireEvent.click(await screen.findByTestId("template-custom"));
  await screen.findByPlaceholderText("Panel title");

  fireEvent.change(screen.getByDisplayValue("Query"), { target: { value: "text" } });

  expect(screen.getByPlaceholderText("Panel text content")).toBeInTheDocument();
  expect(screen.queryByPlaceholderText("Natural language question, e.g. error rate over time")).not.toBeInTheDocument();
});

test("add panel cancel button closes the form", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Add panel" }));
  fireEvent.click(await screen.findByTestId("template-custom"));
  expect(await screen.findByPlaceholderText("Panel title")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByPlaceholderText("Panel title")).not.toBeInTheDocument();
});

test("add panel from template submits pre-filled query panel", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Add panel" }));
  expect(await screen.findByTestId("template-error-rate")).toBeInTheDocument();

  fireEvent.click(screen.getByTestId("template-error-rate"));

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            title: "Error rate",
            panel_kind: "query",
            query_kind: "metrics",
            query_text: "error rate over time",
            layout: expect.objectContaining({ x: 0, y: 4, w: 12, h: 4 }),
          }),
        ]),
      }),
    ),
  );
});

test("Edit layout button enters edit mode showing Done and Cancel", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  expect(screen.queryByRole("button", { name: "Done" })).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));

  expect(screen.getByRole("button", { name: "Done" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Edit layout" })).not.toBeInTheDocument();
});

test("Cancel exits edit mode without calling updateDashboard", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));
  fireEvent.click(screen.getByRole("button", { name: "Simulate layout change" }));
  fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

  expect(screen.getByRole("button", { name: "Edit layout" })).toBeInTheDocument();
  expect(updateSpy).not.toHaveBeenCalled();
});

test("Done saves staged layout to API and exits edit mode", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();
  await screen.findByRole("heading", { name: "Checkout Health" });

  fireEvent.click(screen.getByRole("button", { name: "Edit layout" }));
  fireEvent.click(screen.getByRole("button", { name: "Simulate layout change" }));
  fireEvent.click(screen.getByRole("button", { name: "Done" }));

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            panel_id: "query-1",
            layout: { x: 3, y: 0, w: 6, h: 4 },
          }),
        ]),
      }),
    ),
  );

  await waitFor(() =>
    expect(screen.getByRole("button", { name: "Edit layout" })).toBeInTheDocument(),
  );
});

test("panel shows ErrorState when the panel query fails", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.mocked(submitNlqQuery).mockRejectedValue(new Error("upstream timeout"));

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });
  expect(await screen.findByText("Panel query failed")).toBeInTheDocument();
  expect(await screen.findByText(/upstream timeout/)).toBeInTheDocument();
});

test("panel shows EmptyState when the query returns no frame data", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);
  vi.mocked(submitNlqQuery).mockResolvedValue({ type: "text", text: "no data" } as never);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });
  expect(await screen.findByText("No panel data")).toBeInTheDocument();
});

test("dashboard shows ErrorState when the dashboard fails to load", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockRejectedValue(new Error("dashboard fetch failed"));

  renderPage();

  expect(await screen.findByText("Dashboard could not be loaded")).toBeInTheDocument();
});

test("metrics panels without query text execute a metric catalog base IR", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      {
        ...dashboard.panels[0],
        query_kind: "metrics",
        query_text: undefined,
        service: undefined,
        filters: { name: "request", type: "histogram", environment: "prod" },
      },
    ],
  });
  vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue(dashboard);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });

  await waitFor(() =>
    expect(submitNlqQuery).toHaveBeenCalledWith(
      "test-tenant",
      expect.objectContaining({
        question: undefined,
        mode: "execute",
        base_ir: expect.objectContaining({
          operation: "catalog",
          signals: ["metrics"],
          catalog_field: "metric_name",
          filters: expect.arrayContaining([
            { field: "metric_name", op: "=", value: "request" },
            { field: "metric_type", op: "=", value: "histogram" },
            { field: "environment", op: "=", value: "prod" },
          ]),
        }),
      }),
    ),
  );
});
