import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const TENANT_ID = "00000000-0000-0000-0000-000000000002";

let App: typeof import("../../App").default;

function stubFetch(
  overrides: Record<string, (init?: RequestInit) => Response> = {},
) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      for (const [pattern, handler] of Object.entries(overrides)) {
        if (url.includes(pattern)) return handler(init);
      }
      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: "aaaaaaaa-0000-0000-0000-000000000001",
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ environments: [] }), { status: 200 });
      }
      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/traces")) {
        return new Response(JSON.stringify({ traces: [], total: 0 }), { status: 200 });
      }
      if (url.includes("/v1/logs")) {
        return new Response(JSON.stringify({ logs: [], total: 0 }), { status: 200 });
      }
      if (url.includes("/v1/metrics") && !url.includes("/points")) {
        return new Response(JSON.stringify({ metrics: [] }), { status: 200 });
      }
      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/getting-started");
  vi.resetModules();
  stubFetch();
  ({ default: App } = await import("../../App"));
});

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

test("renders language picker on first visit", async () => {
  render(<App />);
  await screen.findByText("Getting Started");
  expect(screen.getByText("Node.js")).toBeInTheDocument();
  expect(screen.getByText("Python")).toBeInTheDocument();
  expect(screen.getByText("Go")).toBeInTheDocument();
});

test("Next button disabled until language selected", async () => {
  render(<App />);
  await screen.findByText("Getting Started");
  const nextBtn = screen.getByRole("button", { name: "Next →" });
  expect(nextBtn).toBeDisabled();
  fireEvent.click(screen.getByText("Node.js"));
  expect(nextBtn).not.toBeDisabled();
});

test("advances to API key step after language selection", async () => {
  render(<App />);
  await screen.findByText("Getting Started");
  fireEvent.click(screen.getByText("Python"));
  fireEvent.click(screen.getByRole("button", { name: "Next →" }));
  expect(screen.getAllByText("Get API key").length).toBeGreaterThan(0);
  expect(screen.getByRole("button", { name: "Create API key" })).toBeInTheDocument();
});

test("creates token and shows waiting step", async () => {
  stubFetch({
    "/v1/tokens": (init) => {
      if (init?.method === "POST") {
        return new Response(
          JSON.stringify({
            id: "tok-001",
            name: "onboarding-nodejs",
            tenant_name: "observable",
            environment: "production",
            created_at: new Date().toISOString(),
            revoked: false,
            plaintext: "secret-token-abc",
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ tokens: [] }), { status: 200 });
    },
  });
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  render(<App />);
  await screen.findByText("Getting Started");
  fireEvent.click(screen.getByText("Node.js"));
  fireEvent.click(screen.getByRole("button", { name: "Next →" }));
  fireEvent.click(screen.getByRole("button", { name: "Create API key" }));

  await waitFor(() =>
    expect(screen.getByText("secret-token-abc")).toBeInTheDocument(),
  );
  await waitFor(() =>
    expect(screen.getByRole("status")).toBeInTheDocument(),
  );
});

test("shows success state when signals detected", async () => {
  stubFetch({
    "/v1/tokens": (init) => {
      if (init?.method === "POST") {
        return new Response(
          JSON.stringify({
            id: "tok-001",
            name: "onboarding-nodejs",
            tenant_name: "observable",
            environment: "production",
            created_at: new Date().toISOString(),
            revoked: false,
            plaintext: "secret-token-abc",
          }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({ tokens: [] }), { status: 200 });
    },
    "/v1/traces": () =>
      new Response(JSON.stringify({ traces: [{ trace_id: "t1" }], total: 1 }), {
        status: 200,
      }),
  });
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  render(<App />);
  await screen.findByText("Getting Started");
  fireEvent.click(screen.getByText("Node.js"));
  fireEvent.click(screen.getByRole("button", { name: "Next →" }));
  fireEvent.click(screen.getByRole("button", { name: "Create API key" }));

  await waitFor(
    () => expect(screen.getByText(/first signal arrived/i)).toBeInTheDocument(),
    { timeout: 10000 },
  );
});

test("skip wizard sets complete flag", async () => {
  render(<App />);
  await screen.findByText("Getting Started");

  fireEvent.click(screen.getByRole("button", { name: "Skip wizard" }));

  const stored = JSON.parse(localStorage.getItem("observable_onboarding") ?? "{}");
  expect(stored.complete).toBe(true);
});
