import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import App from "./App";
import { THEME_STORAGE_KEY } from "./lib/theme";

beforeEach(() => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/");
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/services/summary")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }

      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }

      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
});

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders the product navigation shell", async () => {
  render(<App />);

  const navigation = await screen.findByLabelText("Primary navigation");
  expect(within(navigation).getByRole("link", { name: "Services" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Infrastructure" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Service Overview" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Dashboards" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Alerts & SLOs" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Admin / Fleet / Billing" })).toBeInTheDocument();

  await screen.findByRole("heading", { name: "Services" });
});

test("persists the selected theme preference", async () => {
  render(<App />);

  fireEvent.click(screen.getByRole("radio", { name: "Dark" }));

  expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
  expect(document.documentElement.dataset.themePreference).toBe("dark");
  expect(document.documentElement.dataset.theme).toBe("dark");
});

test("renders the service detail overview with performance entry points", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/services/checkout/summary")) {
        return new Response(
          JSON.stringify({
            service: {
              service_name: "checkout",
              request_rate: 12.5,
              error_rate: 0.025,
              p95_latency_ms: 245,
              health_state: "watch",
              active_alert_count: 2,
              latest_deployment: "checkout@2026.04.21",
            },
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/services/checkout");

  render(<App />);

  await screen.findByRole("heading", { name: "checkout" });
  expect(screen.getByText("12.50 rps")).toBeInTheDocument();
  expect(screen.getByText("2.50%")).toBeInTheDocument();
  expect(screen.getByText("245ms")).toBeInTheDocument();
  expect(screen.getByText("checkout@2026.04.21")).toBeInTheDocument();
  const entryPoints = screen.getByLabelText("Signal entry points");
  expect(within(entryPoints).getByRole("link", { name: "Traces" })).toHaveAttribute(
    "href",
    "/traces?service=checkout",
  );
  expect(within(entryPoints).getByRole("link", { name: "Infrastructure" })).toHaveAttribute(
    "href",
    "/infrastructure?service=checkout",
  );
});

test("renders service-scoped signal tabs with preserved URL state", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = String(input);

    if (url.includes("/v1/services/checkout/summary")) {
      return new Response(
        JSON.stringify({
          service: {
            service_name: "checkout",
            request_rate: 12.5,
            error_rate: 0.025,
            p95_latency_ms: 245,
            health_state: "watch",
            active_alert_count: 2,
            latest_deployment: "checkout@2026.04.21",
          },
        }),
        { status: 200 },
      );
    }

    if (url.includes("/v1/logs")) {
      return new Response(
        JSON.stringify({
          logs: [
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              log_id: "00000000-0000-0000-0000-000000000111",
              timestamp_unix_nano: "10",
              severity_number: 9,
              severity_text: "INFO",
              body: "cart accepted",
              service_name: "checkout",
            },
          ],
          total: 1,
        }),
        { status: 200 },
      );
    }

    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });

  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/services/checkout/logs?lookback_minutes=60");

  render(<App />);

  const tabs = await screen.findByLabelText("Service signals");
  expect(within(tabs).getByRole("link", { name: "Logs" })).toHaveAttribute(
    "href",
    "/services/checkout/logs?lookback_minutes=60",
  );
  expect(within(tabs).getByRole("link", { name: "Metrics" })).toHaveAttribute(
    "href",
    "/services/checkout/metrics?lookback_minutes=60",
  );
  expect(await screen.findByText("cart accepted")).toBeInTheDocument();
  expect(fetchMock).toHaveBeenCalledWith(
    expect.stringContaining("/v1/logs?service=checkout&lookback_minutes=60&limit=50"),
    expect.anything(),
  );
});

test("browser back restores the previous service signal tab", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/services/checkout/summary")) {
        return new Response(
          JSON.stringify({
            service: {
              service_name: "checkout",
              request_rate: 12.5,
              error_rate: 0.025,
              p95_latency_ms: 245,
              health_state: "watch",
              active_alert_count: 2,
              latest_deployment: "checkout@2026.04.21",
            },
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/metrics")) {
        return new Response(
          JSON.stringify({
            series: [
              {
                tenant_id: "00000000-0000-0000-0000-000000000001",
                metric_series_id: "00000000-0000-0000-0000-000000000222",
                metric_name: "checkout.requests",
                description: "",
                unit: "1",
                metric_type: "sum",
                attributes: {},
                resource_attributes: {},
                service_name: "checkout",
                environment: "prod",
              },
            ],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/traces")) {
        return new Response(
          JSON.stringify({
            traces: [
              {
                trace_id: "abcdef0123456789",
                spans: [
                  {
                    tenant_id: "00000000-0000-0000-0000-000000000001",
                    trace_id: "abcdef0123456789",
                    span_id: "span-1",
                    service_name: "checkout",
                    operation_name: "POST /checkout",
                    start_time_unix_nano: 0,
                    end_time_unix_nano: 1000000,
                    duration_ns: 1000000,
                    status_code: "OK",
                  },
                ],
              },
            ],
            total: 1,
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/services/checkout/metrics?lookback_minutes=60");

  render(<App />);

  expect(await screen.findByText("checkout.requests")).toBeInTheDocument();
  fireEvent.click(within(screen.getByLabelText("Service signals")).getByRole("link", { name: "Traces" }));
  expect(await screen.findByText("POST /checkout")).toBeInTheDocument();

  window.history.back();

  await waitFor(() => expect(screen.getByText("checkout.requests")).toBeInTheDocument());
});
