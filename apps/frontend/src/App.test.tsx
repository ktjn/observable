import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { THEME_STORAGE_KEY } from "./lib/theme";

let App: typeof import("./App").default;

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/");
  vi.resetModules();
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
  ({ default: App } = await import("./App"));
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

  const darkTheme = await screen.findByRole("radio", { name: "Dark" });
  fireEvent.click(darkTheme);

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

test("renders infrastructure inventory rows from the infrastructure API", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                entity_type: "pod",
                entity_id: "prod-cluster/payments/checkout-pod-1",
                display_name: "checkout-pod-1",
                parent_id: "payments",
                parent_display_name: "payments",
                environment: "prod",
                health_state: "watch",
                last_seen_unix_nano: 42,
                related_services: ["checkout-api"],
                log_rate_per_minute: 8.5,
                error_rate: 0.02,
                restart_count: null,
                cpu_usage: null,
                memory_usage: null,
                disk_usage: null,
                network_io: null,
              },
            ],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Infrastructure" })).toBeInTheDocument();
  expect(await screen.findByText("checkout-pod-1")).toBeInTheDocument();
  expect(screen.getByText("checkout-api")).toBeInTheDocument();
});

test("filters the infrastructure inventory by entity type", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                entity_type: "pod",
                entity_id: "prod-cluster/payments/checkout-pod-1",
                display_name: "checkout-pod-1",
                parent_id: "payments",
                parent_display_name: "payments",
                environment: "prod",
                health_state: "watch",
                last_seen_unix_nano: 42,
                related_services: ["checkout-api"],
                log_rate_per_minute: 8.5,
                error_rate: 0.02,
                restart_count: null,
                cpu_usage: null,
                memory_usage: null,
                disk_usage: null,
                network_io: null,
              },
              {
                entity_type: "host",
                entity_id: "ip-10-0-0-12",
                display_name: "ip-10-0-0-12",
                parent_id: null,
                parent_display_name: null,
                environment: "prod",
                health_state: "healthy",
                last_seen_unix_nano: 43,
                related_services: ["checkout-api"],
                log_rate_per_minute: 1.5,
                error_rate: null,
                restart_count: null,
                cpu_usage: 0.27,
                memory_usage: 0.61,
                disk_usage: null,
                network_io: null,
              },
            ],
          }),
          { status: 200 },
        );
      }

      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  await screen.findByText("checkout-pod-1");
  fireEvent.change(screen.getByLabelText("Infrastructure type filter"), {
    target: { value: "host" },
  });

  expect(screen.queryByText("checkout-pod-1")).not.toBeInTheDocument();
  expect(screen.getByText("ip-10-0-0-12")).toBeInTheDocument();
});

test("renders infrastructure detail action links from a pod detail route", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1")) {
        return new Response(
          JSON.stringify({
            entity: {
              entity_type: "pod",
              entity_id: "prod-cluster/payments/checkout-pod-1",
              display_name: "checkout-pod-1",
              parent_id: "payments",
              parent_display_name: "payments",
              environment: "prod",
              health_state: "watch",
              last_seen_unix_nano: 42,
              related_services: ["checkout-api"],
              log_rate_per_minute: 8.5,
              error_rate: 0.02,
              restart_count: null,
              cpu_usage: null,
              memory_usage: null,
              disk_usage: null,
              network_io: null,
            },
            links: {
              logs: "/logs?resource_attr=k8s.pod.name:prod-cluster/payments/checkout-pod-1",
              traces: "/traces?resource_attr=k8s.pod.name:prod-cluster/payments/checkout-pod-1",
              metrics:
                "/services/checkout-api/metrics?resource_attr=k8s.pod.name:prod-cluster/payments/checkout-pod-1",
            },
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState(
    {},
    "",
    "/infrastructure/pod/prod-cluster%2Fpayments%2Fcheckout-pod-1",
  );
  render(<App />);

  expect(await screen.findByRole("heading", { name: "checkout-pod-1" })).toBeInTheDocument();
  expect(within(screen.getByLabelText("Infrastructure action links")).getByRole("link", { name: "Logs" })).toHaveAttribute(
    "href",
    "/logs?resource_attr=k8s.pod.name:prod-cluster/payments/checkout-pod-1",
  );
});

test("renders empty state when infrastructure inventory has no items", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/infrastructure")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Infrastructure" })).toBeInTheDocument();
  expect(
    await screen.findByText("No infrastructure entities matched the current filters."),
  ).toBeInTheDocument();
});

test("navigates to infrastructure detail when clicking an inventory row entity", async () => {
  const podItem = {
    entity_type: "pod",
    entity_id: "prod-cluster/payments/checkout-pod-1",
    display_name: "checkout-pod-1",
    parent_id: "prod-cluster/payments",
    parent_display_name: "payments",
    environment: "prod",
    health_state: "watch",
    last_seen_unix_nano: 42,
    related_services: ["checkout-api"],
    log_rate_per_minute: 8.5,
    error_rate: 0.02,
    restart_count: null,
    cpu_usage: null,
    memory_usage: null,
    disk_usage: null,
    network_io: null,
  };

  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/infrastructure/pod/")) {
        return new Response(
          JSON.stringify({
            entity: podItem,
            links: {
              logs: "/logs?resource_attr=k8s.pod.name%3Acheckout-pod-1",
              traces: "/traces?resource_attr=k8s.pod.name%3Acheckout-pod-1",
              metrics:
                "/services/checkout-api/metrics?resource_attr=k8s.pod.name%3Acheckout-pod-1",
            },
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/infrastructure")) {
        return new Response(JSON.stringify({ items: [podItem] }), { status: 200 });
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/infrastructure");
  render(<App />);

  const entityLink = await screen.findByRole("link", { name: "checkout-pod-1" });
  fireEvent.click(entityLink);

  expect(await screen.findByLabelText("Infrastructure action links")).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "checkout-pod-1" })).toBeInTheDocument();
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

test("renders service nodes from topology data", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Service Overview" })).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: "checkout-api" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "payments-api" })).toBeInTheDocument();
});

test("clicking a node enters focused mode", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
              {
                caller: "gateway",
                callee: "checkout-api",
                request_count: 200,
                error_rate: 0.0,
                p95_latency_ms: 10.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  const checkoutNode = await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(checkoutNode);

  expect(screen.getByText("Viewing: checkout-api")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "→ Service detail" })).toBeInTheDocument();
});

test("clicking a focused node returns to full graph", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  const checkoutNode = await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(checkoutNode);
  expect(screen.getByText("Viewing: checkout-api")).toBeInTheDocument();

  fireEvent.click(checkoutNode);
  expect(screen.queryByText("Viewing: checkout-api")).not.toBeInTheDocument();
});

test("clicking an edge shows trace and log links", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  await screen.findByRole("button", { name: "checkout-api" });
  const edgeButton = screen.getByRole("button", { name: "checkout-api to payments-api" });
  fireEvent.click(edgeButton);

  const tracesLink = screen.getByRole("link", { name: "View Traces" });
  expect(tracesLink).toHaveAttribute(
    "href",
    "/traces?caller=checkout-api&callee=payments-api&lookback_minutes=60",
  );
  const logsLink = screen.getByRole("link", { name: "View Logs" });
  expect(logsLink).toHaveAttribute("href", "/logs?service=checkout-api&lookback_minutes=60");
});

test("clicking SVG background closes edge popover", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(
          JSON.stringify({
            edges: [
              {
                caller: "checkout-api",
                callee: "payments-api",
                request_count: 100,
                error_rate: 0.01,
                p95_latency_ms: 45.0,
              },
            ],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: ["prod"] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  await screen.findByRole("button", { name: "checkout-api" });
  fireEvent.click(screen.getByRole("button", { name: "checkout-api to payments-api" }));
  expect(screen.getByRole("link", { name: "View Traces" })).toBeInTheDocument();

  fireEvent.click(screen.getByTestId("topology-background"));
  expect(screen.queryByRole("link", { name: "View Traces" })).not.toBeInTheDocument();
});

test("renders empty state when no edges returned", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/topology")) {
        return new Response(JSON.stringify({ edges: [] }), { status: 200 });
      }
      if (url.includes("/v1/environments")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );

  window.history.pushState({}, "", "/service-overview");
  render(<App />);

  expect(
    await screen.findByText("No service relationships found in the selected lookback."),
  ).toBeInTheDocument();
});
