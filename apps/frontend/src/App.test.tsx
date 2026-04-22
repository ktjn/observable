import { fireEvent, render, screen, within } from "@testing-library/react";
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
  expect(screen.getByRole("link", { name: "Traces" })).toHaveAttribute(
    "href",
    "/traces?service=checkout",
  );
  const entryPoints = screen.getByLabelText("Signal entry points");
  expect(within(entryPoints).getByRole("link", { name: "Infrastructure" })).toHaveAttribute(
    "href",
    "/infrastructure?service=checkout",
  );
});
