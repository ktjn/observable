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
  expect(within(navigation).getByRole("link", { name: "Setup" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Services" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Infrastructure" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Service Overview" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Dashboards" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Alerts & SLOs" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Admin / Fleet / Billing" })).toBeInTheDocument();

  await screen.findByRole("heading", { name: "Services" });
});

test("promotes the current log search filter to a dashboard panel", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);

    if (url.includes("/v1/logs/histogram")) {
      return new Response(JSON.stringify({ buckets: [] }), { status: 200 });
    }

    if (url.includes("/v1/logs")) {
      return new Response(
        JSON.stringify({
          logs: [
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              log_id: "log-1",
              timestamp_unix_nano: "1700000000000000000",
              severity_number: 9,
              severity_text: "INFO",
              body: "checkout complete",
              service_name: "checkout",
              resource_attributes: {},
            },
          ],
          total: 1,
          facets: {},
        }),
        { status: 200 },
      );
    }

    if (url.includes("/v1/dashboards") && init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      expect(body.panels[0]).toMatchObject({
        query_kind: "logs",
        service: "checkout",
        lookback_minutes: 60,
      });
      return new Response(
        JSON.stringify({
          dashboard_id: "dash-1",
          name: "Promoted log query",
          panels: body.panels.map((panel: object, index: number) => ({
            panel_id: `panel-${index + 1}`,
            ...panel,
          })),
          created_at: "2026-04-29T00:00:00Z",
        }),
        { status: 201 },
      );
    }

    if (url.includes("/v1/dashboards")) {
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }

    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/logs?service=checkout");

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Logs" })).toBeInTheDocument();
  expect(await screen.findByText("checkout complete")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Promote to dashboard" }));

  expect(await screen.findByText("Saved to dashboard")).toBeInTheDocument();
  expect(fetchMock).toHaveBeenCalledWith(
    expect.stringContaining("/v1/dashboards"),
    expect.objectContaining({ method: "POST" }),
  );
});

test("promotes the current trace search filter to a dashboard panel", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);

    if (url.includes("/v1/traces/histogram")) {
      return new Response(JSON.stringify({ buckets: [] }), { status: 200 });
    }

    if (url.includes("/v1/traces")) {
      return new Response(
        JSON.stringify({
          traces: [
            {
              trace_id: "trace-1",
              spans: [
                {
                  tenant_id: "00000000-0000-0000-0000-000000000001",
                  trace_id: "trace-1",
                  span_id: "span-1",
                  service_name: "checkout",
                  operation_name: "GET /checkout",
                  start_time_unix_nano: 1,
                  end_time_unix_nano: 2,
                  duration_ns: 1000000,
                  status_code: "OK",
                  resource_attributes: {},
                },
              ],
            },
          ],
          total: 1,
          facets: {},
        }),
        { status: 200 },
      );
    }

    if (url.includes("/v1/dashboards") && init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      expect(body.panels[0]).toMatchObject({
        query_kind: "traces",
        service: "checkout",
        lookback_minutes: 60,
      });
      return new Response(
        JSON.stringify({
          dashboard_id: "dash-1",
          name: "Promoted trace query",
          panels: body.panels.map((panel: object, index: number) => ({
            panel_id: `panel-${index + 1}`,
            ...panel,
          })),
          created_at: "2026-04-29T00:00:00Z",
        }),
        { status: 201 },
      );
    }

    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/traces?service=checkout");

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Traces" })).toBeInTheDocument();
  expect(await screen.findByText("GET /checkout")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Promote to dashboard" }));

  expect(await screen.findByText("Saved to dashboard")).toBeInTheDocument();
});

test("renders saved dashboard panels with preserved query context", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/dashboards")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                dashboard_id: "dash-1",
                name: "Promoted log query",
                panels: [
                  {
                    panel_id: "panel-1",
                    title: "Logs for checkout",
                    query_kind: "logs",
                    service: "checkout",
                    lookback_minutes: 60,
                    filters: { facets: ["service_name", "severity_number"] },
                  },
                ],
                created_at: "2026-04-29T00:00:00Z",
              },
            ],
          }),
          { status: 200 },
        );
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/dashboards");

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Dashboards" })).toBeInTheDocument();
  expect(await screen.findByText("Promoted log query")).toBeInTheDocument();
  expect(screen.getByText("Logs for checkout")).toBeInTheDocument();
  expect(screen.getByText("logs · checkout · Last 60m")).toBeInTheDocument();
});

test("renders onboarding setup with endpoint, redacted key, and first signal success", async () => {
  const writeText = vi.fn(async () => undefined);
  Object.assign(navigator, { clipboard: { writeText } });
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);

      if (url.includes("/v1/traces")) {
        return new Response(JSON.stringify({ traces: [{ trace_id: "abc", spans: [] }], total: 1 }), {
          status: 200,
        });
      }

      if (url.includes("/v1/logs/histogram")) {
        return new Response(JSON.stringify({ buckets: [] }), { status: 200 });
      }

      if (url.includes("/v1/logs")) {
        return new Response(JSON.stringify({ logs: [], total: 0 }), { status: 200 });
      }

      if (url.includes("/v1/metrics")) {
        return new Response(JSON.stringify({ series: [] }), { status: 200 });
      }

      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/setup");

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Setup" })).toBeInTheDocument();
  expect(screen.getByText("http://localhost:4318/v1/traces")).toBeInTheDocument();
  expect(screen.getByText("00000000-0000-0000-0000-000000000001")).toBeInTheDocument();
  expect(screen.getByText("dev-api-key-...-0000")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Copy API key" }));
  expect(writeText).toHaveBeenCalledWith("dev-api-key-0000");
  expect(await screen.findByText("First signal detected")).toBeInTheDocument();
});

test("renders first signal empty state when no telemetry is queryable yet", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify({ traces: [], logs: [], series: [], total: 0 }), { status: 200 })),
  );
  window.history.pushState({}, "", "/setup");

  render(<App />);

  expect(await screen.findByRole("heading", { name: "Setup" })).toBeInTheDocument();
  expect(await screen.findByText("Waiting for first signal")).toBeInTheDocument();
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
  expect(within(entryPoints).getByRole("link", { name: "Metrics" })).toHaveAttribute(
    "href",
    "/services/checkout/metrics?lookback_minutes=60",
  );
  expect(within(entryPoints).getByRole("link", { name: "Infrastructure" })).toHaveAttribute(
    "href",
    "/infrastructure?service=checkout",
  );
});

test("renders service metrics workspace with filtering and selected series points", async () => {
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

    if (url.includes("/v1/metrics/00000000-0000-0000-0000-000000000222")) {
      return new Response(
        JSON.stringify({
          points: [
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              metric_series_id: "00000000-0000-0000-0000-000000000222",
              metric_name: "checkout.requests",
              service_name: "checkout",
              time_unix_nano: 1700000000000000000,
              start_time_unix_nano: null,
              value_double: 42.5,
              value_int: null,
              histogram_count: null,
              histogram_sum: null,
              histogram_bucket_counts: [],
              histogram_explicit_bounds: [],
            },
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              metric_series_id: "00000000-0000-0000-0000-000000000222",
              metric_name: "checkout.requests",
              service_name: "checkout",
              time_unix_nano: 1700000060000000000,
              start_time_unix_nano: null,
              value_double: 47,
              value_int: null,
              histogram_count: null,
              histogram_sum: null,
              histogram_bucket_counts: [],
              histogram_explicit_bounds: [],
            },
          ],
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
              is_monotonic: true,
              aggregation_temporality: "cumulative",
              attributes: { route: "/checkout" },
              resource_attributes: { "k8s.namespace.name": "payments" },
              service_name: "checkout",
              environment: "prod",
            },
            {
              tenant_id: "00000000-0000-0000-0000-000000000001",
              metric_series_id: "00000000-0000-0000-0000-000000000333",
              metric_name: "checkout.latency",
              description: "",
              unit: "ms",
              metric_type: "gauge",
              attributes: { route: "/checkout" },
              resource_attributes: {},
              service_name: "checkout",
              environment: "stage",
            },
          ],
        }),
        { status: 200 },
      );
    }

    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/services/checkout/metrics?lookback_minutes=60");

  render(<App />);

  expect(await screen.findByText("2 series")).toBeInTheDocument();
  expect(screen.getByText("2 types")).toBeInTheDocument();
  expect(screen.getByText("2 envs")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Metric name filter"), {
    target: { value: "latency" },
  });
  expect(screen.queryByText("checkout.requests")).not.toBeInTheDocument();
  expect(screen.getByText("checkout.latency")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Metric name filter"), {
    target: { value: "" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Select checkout.requests" }));

  expect(await screen.findByText("Selected series")).toBeInTheDocument();
  await waitFor(() => expect(screen.getAllByText("2 points").length).toBeGreaterThan(0));
  await waitFor(() => expect(screen.getAllByText("47").length).toBeGreaterThan(0));
  expect(fetchMock).toHaveBeenCalledWith(
    expect.stringContaining("/v1/metrics/00000000-0000-0000-0000-000000000222"),
    expect.anything(),
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

    if (url.includes("/v1/logs/histogram")) {
      return new Response(JSON.stringify({ buckets: [] }), { status: 200 });
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
  window.history.pushState({}, "", "/services/checkout/logs");

  render(<App />);

  const tabs = await screen.findByLabelText("Service signals");
  expect(within(tabs).getByRole("link", { name: "Logs" })).toHaveAttribute(
    "href",
    "/services/checkout/logs",
  );
  expect(within(tabs).getByRole("link", { name: "Metrics" })).toHaveAttribute(
    "href",
    "/services/checkout/metrics",
  );
  expect(await screen.findByText("cart accepted")).toBeInTheDocument();
  expect(fetchMock).toHaveBeenCalledWith(
    expect.stringContaining("/v1/logs?service=checkout&from="),
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

test("alerts page renders rule list with firing badge", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/alerts/rules") && !url.includes("silence")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                rule_id: "10000000-0000-0000-0000-000000000001",
                name: "High error rate",
                metric_name: "error_rate",
                operator: "gt",
                threshold: 0.05,
                severity: "warning",
                silenced: false,
                firing: true,
                last_fired_at: "2026-04-28T10:00:00Z",
              },
            ],
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByRole("heading", { name: "Alerts & SLOs" })).toBeInTheDocument();
  expect(await screen.findByText("High error rate")).toBeInTheDocument();
  expect(screen.getAllByText("Firing").length).toBeGreaterThan(0);
  expect(screen.getByRole("button", { name: "Silence" })).toBeInTheDocument();
});

test("alerts page shows OK status when rule is not firing", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/alerts/rules")) {
        return new Response(
          JSON.stringify({
            items: [
              {
                rule_id: "10000000-0000-0000-0000-000000000002",
                name: "Low traffic",
                metric_name: "requests",
                operator: "lt",
                threshold: 1.0,
                severity: "warning",
                silenced: false,
                firing: false,
                last_fired_at: null,
              },
            ],
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByText("OK")).toBeInTheDocument();
});

test("alerts page silence button calls PATCH and refreshes list", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";

    if (url.includes("/v1/alerts/rules") && url.includes("silence") && method === "PATCH") {
      return new Response(
        JSON.stringify({
          rule_id: "10000000-0000-0000-0000-000000000001",
          name: "High error rate",
          metric_name: "error_rate",
          operator: "gt",
          threshold: 0.05,
          severity: "warning",
          silenced: true,
          firing: false,
          last_fired_at: null,
        }),
        { status: 200 },
      );
    }
    if (url.includes("/v1/alerts/rules")) {
      return new Response(
        JSON.stringify({
          items: [
            {
              rule_id: "10000000-0000-0000-0000-000000000001",
              name: "High error rate",
              metric_name: "error_rate",
              operator: "gt",
              threshold: 0.05,
              severity: "warning",
              silenced: false,
              firing: true,
              last_fired_at: null,
            },
          ],
        }),
        { status: 200 },
      );
    }
    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  const silenceBtn = await screen.findByRole("button", { name: "Silence" });
  fireEvent.click(silenceBtn);

  await waitFor(() => {
    const patchCall = fetchMock.mock.calls.find(
      ([url, init]) =>
        String(url).includes("silence") && (init as RequestInit)?.method === "PATCH",
    );
    expect(patchCall).toBeDefined();
    const body = JSON.parse((patchCall![1] as RequestInit).body as string);
    expect(body.silenced).toBe(true);
  });
});

test("alerts page create form submits POST and closes panel", async () => {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";

    if (url.includes("/v1/alerts/rules") && method === "POST") {
      return new Response(
        JSON.stringify({
          rule_id: "20000000-0000-0000-0000-000000000001",
          name: "High latency",
          metric_name: "p95_latency_ms",
          operator: "gt",
          threshold: 500,
          severity: "warning",
          silenced: false,
          firing: false,
          last_fired_at: null,
        }),
        { status: 201 },
      );
    }
    if (url.includes("/v1/alerts/rules")) {
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }
    return new Response(JSON.stringify({ items: [] }), { status: 200 });
  });
  vi.stubGlobal("fetch", fetchMock);
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  await screen.findByRole("heading", { name: "Alerts & SLOs" });

  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));
  expect(screen.getByLabelText("Create alert rule")).toBeInTheDocument();

  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "High latency" },
  });
  fireEvent.change(screen.getByLabelText("Metric name"), {
    target: { value: "p95_latency_ms" },
  });
  fireEvent.change(screen.getByLabelText("Threshold value"), {
    target: { value: "500" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => {
    const postCall = fetchMock.mock.calls.find(
      ([url, init]) =>
        String(url).includes("/v1/alerts/rules") && (init as RequestInit)?.method === "POST",
    );
    expect(postCall).toBeDefined();
    const body = JSON.parse((postCall![1] as RequestInit).body as string);
    expect(body.name).toBe("High latency");
    expect(body.metric_name).toBe("p95_latency_ms");
    expect(body.threshold).toBe(500);
  });

  // Panel should close after success
  await waitFor(() =>
    expect(screen.queryByLabelText("Create alert rule")).not.toBeInTheDocument(),
  );
});

test("alerts page renders empty state when no rules exist", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      if (String(input).includes("/v1/alerts/rules")) {
        return new Response(JSON.stringify({ items: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({ items: [] }), { status: 200 });
    }),
  );
  window.history.pushState({}, "", "/alerts");
  render(<App />);

  expect(await screen.findByText("No alert rules")).toBeInTheDocument();
  expect(
    screen.getByText("Create a threshold rule to start monitoring metrics."),
  ).toBeInTheDocument();
});
