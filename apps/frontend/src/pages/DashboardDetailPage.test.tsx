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
    nlq_ir: { operation: "table" },
    source_sql: "select 1",
    time_range: { from: "1", to: "2" },
    signal_types: ["logs"],
    sample_rate: null,
    approximation_statement: "Approximate operational data.",
  },
};

const dashboard: dashboardsApi.Dashboard = {
  dashboard_id: "dash-1",
  name: "Checkout Health",
  created_at: "2026-05-10T00:00:00Z",
  panels: [
    {
      panel_id: "query-1",
      title: "Error logs",
      panel_kind: "query",
      query_kind: "logs",
      service: "checkout",
      preset: null,
      filters: {},
      query_text: "errors in checkout",
      content: null,
      layout: { x: 0, y: 0, w: 6, h: 4 },
      time_range: { mode: "global" },
    },
    {
      panel_id: "text-1",
      title: "Incident notes",
      panel_kind: "text",
      query_kind: null,
      service: null,
      preset: null,
      filters: {},
      query_text: null,
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

test("dragging a panel left border persists a new layout width", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      dashboard.panels[0],
      { ...dashboard.panels[1], layout: { x: 5, y: 0, w: 7, h: 2 } },
    ],
  });
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });
  const handle = screen.getByLabelText("Resize Incident notes from left border");
  fireEvent.pointerDown(handle, { clientX: 240, pointerId: 1 });
  fireEvent.pointerMove(document, { clientX: 160, pointerId: 1 });
  fireEvent.pointerUp(document, { clientX: 160, pointerId: 1 });

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            panel_id: "text-1",
            layout: expect.objectContaining({ x: 5, w: 7 }),
          }),
        ]),
      }),
    ),
  );
});

test("dragging a panel right border persists a new layout width", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      { ...dashboard.panels[0], layout: { x: 0, y: 0, w: 7, h: 4 } },
      dashboard.panels[1],
    ],
  });
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });
  const handle = screen.getByLabelText("Resize Error logs from right border");
  fireEvent.pointerDown(handle, { clientX: 100, pointerId: 1 });
  fireEvent.pointerMove(document, { clientX: 180, pointerId: 1 });
  fireEvent.pointerUp(document, { clientX: 180, pointerId: 1 });

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            panel_id: "query-1",
            layout: expect.objectContaining({ x: 0, w: 7 }),
          }),
        ]),
      }),
    ),
  );
});

test("dragging a panel bottom border persists a new layout height", async () => {
  const updateSpy = vi.spyOn(dashboardsApi, "updateDashboard").mockResolvedValue({
    ...dashboard,
    panels: [{ ...dashboard.panels[0], layout: { x: 0, y: 0, w: 6, h: 5 } }, dashboard.panels[1]],
  });
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue(dashboard);

  renderPage();

  await screen.findByRole("heading", { name: "Checkout Health" });
  const handle = screen.getByLabelText("Resize Error logs from bottom border");
  fireEvent.pointerDown(handle, { clientY: 320, pointerId: 1 });
  fireEvent.pointerMove(document, { clientY: 410, pointerId: 1 });
  fireEvent.pointerUp(document, { clientY: 410, pointerId: 1 });

  await waitFor(() =>
    expect(updateSpy).toHaveBeenCalledWith(
      "test-tenant",
      "dash-1",
      expect.objectContaining({
        panels: expect.arrayContaining([
          expect.objectContaining({
            panel_id: "query-1",
            layout: expect.objectContaining({ h: 5 }),
          }),
        ]),
      }),
    ),
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

test("metrics panels without query text execute a metric catalog base IR", async () => {
  vi.spyOn(dashboardsApi, "getDashboard").mockResolvedValue({
    ...dashboard,
    panels: [
      {
        ...dashboard.panels[0],
        query_kind: "metrics",
        query_text: null,
        service: null,
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
