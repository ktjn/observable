import { fireEvent, render, screen, waitFor } from "@testing-library/react";

const TENANT_ID = "00000000-0000-0000-0000-000000000002";
const ALICE_ID = "aaaaaaaa-0000-0000-0000-000000000001";
const BOB_ID = "bbbbbbbb-0000-0000-0000-000000000001";

const BASE_MEMBERS = [
  {
    user_id: ALICE_ID,
    email: "alice@example.com",
    name: "Alice",
    role: "tenant_admin",
    joined_at: "2026-01-01T00:00:00Z",
  },
  {
    user_id: BOB_ID,
    email: "bob@example.com",
    name: "Bob",
    role: "member",
    joined_at: "2026-01-02T00:00:00Z",
  },
];

let App: typeof import("../../App").default;

function stubFetch(overrides: Record<string, (init?: RequestInit) => Response> = {}) {
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
            user_id: ALICE_ID,
            email: "alice@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "tenant_admin" }],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/admin/members")) {
        return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
      }
      if (url.includes("/v1/tenants/") && url.includes("/environments")) {
        return new Response(JSON.stringify({ environments: [] }), { status: 200 });
      }
      if (url.includes("/v1/tenants/usage-report")) {
        return new Response(
          JSON.stringify({
            tenant_id: TENANT_ID,
            from: "",
            to: "",
            telemetry_summary: { spans: 0, logs: 0, metric_points: 0, metric_series_created: 0 },
            control_plane_summary: {
              query_reads: 0,
              query_rows: 0,
              credential_checks: 0,
              credential_allows: 0,
              credential_denies: 0,
            },
            estimated_cost_index: 0,
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/admin");
  vi.resetModules();
  stubFetch();
  ({ default: App } = await import("../../App"));
});

afterEach(() => {
  vi.unstubAllGlobals();
});

test("renders member list with roles", async () => {
  window.history.pushState({}, "", "/admin/members");
  render(<App />);
  // Wait for data + tenant context to settle
  await screen.findByLabelText("Email");
  // Alice's name shows with "you" badge; check email instead
  expect(screen.getAllByText("alice@example.com").length).toBeGreaterThan(0);
  expect(screen.getByText("Bob")).toBeInTheDocument();
  expect(screen.getByText("Members (2)")).toBeInTheDocument();
});

test("shows add form for tenant_admin", async () => {
  window.history.pushState({}, "", "/admin/members");
  render(<App />);
  // Wait for the add form to appear — isAdmin resolves after tenants query settles
  await screen.findByLabelText("Email");
  expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
  expect(screen.getByText("Members (2)")).toBeInTheDocument();
});

test("shows inline error when email not found", async () => {
  stubFetch({
    "/v1/admin/members": (init) => {
      if (init?.method === "POST") {
        return new Response(JSON.stringify({ error: "not found" }), { status: 404 });
      }
      return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
    },
  });
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  window.history.pushState({}, "", "/admin/members");
  render(<App />);
  await screen.findByLabelText("Email");
  fireEvent.change(screen.getByLabelText("Email"), {
    target: { value: "nobody@example.com" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Add" }));
  await waitFor(() =>
    expect(screen.getByText("No account found for that email.")).toBeInTheDocument(),
  );
});

test("hides mutation controls for non-admin user", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("/v1/auth/me")) {
        return new Response(
          JSON.stringify({
            user_id: BOB_ID,
            email: "bob@example.com",
            tenants: [{ tenant_id: TENANT_ID, role: "member" }],
          }),
          { status: 200 },
        );
      }
      if (url.includes("/v1/admin/members")) {
        return new Response(JSON.stringify({ members: BASE_MEMBERS }), { status: 200 });
      }
      if (url.includes("/v1/tenants")) {
        return new Response(
          JSON.stringify({ tenants: [{ id: TENANT_ID, name: "observable" }] }),
          { status: 200 },
        );
      }
      return new Response(JSON.stringify({}), { status: 200 });
    }),
  );
  vi.resetModules();
  ({ default: App } = await import("../../App"));

  window.history.pushState({}, "", "/admin/members");
  render(<App />);
  // Wait for member list to load — non-admin sees table but no form
  await screen.findByText("Members (2)");
  expect(screen.queryByLabelText("Email")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: /Remove/ })).not.toBeInTheDocument();
});
