import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import type { IncidentDetailResponse } from "../../api/incidents";
import type { TopologyResponse } from "../../api/services";
import { IncidentDetailPage } from "./IncidentDetailPage";
import type { RuntimeApi } from "../../runtime/types";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../hooks/useAuth", () => ({
  useAuth: () => ({
    data: {
      user_id: "u-1",
      email: "oncall@test.local",
      tenants: [{ tenant_id: "test-tenant", role: "admin" }],
    },
    isLoading: false,
  }),
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
    useParams: () => ({ incidentId: "inc-1" }),
    Link: ({
      children,
      to,
      params,
      ...props
    }: {
      children?: React.ReactNode;
      to: string;
      params?: Record<string, string>;
    } & React.AnchorHTMLAttributes<HTMLAnchorElement>) => (
      <a href={params ? `${to}/${Object.values(params)[0]}` : to} {...props}>
        {children}
      </a>
    ),
  };
});

import { useRuntime } from "../../hooks/useRuntime";

const baseDetail: IncidentDetailResponse = {
  incident_id: "inc-1",
  title: "CPU spike",
  severity: "critical",
  status: "triggered",
  dedup_key: "rule-abc",
  triggered_at: "2026-05-18T10:00:00Z",
  resolved_at: null,
  triggered_by_rule_id: "rule-abc",
  runbook_url: null,
  rule_name: "High CPU Alert",
  impacted_service: null,
  timeline: [
    {
      event_time: "2026-05-18T10:00:01Z",
      event_type: "triggered",
      actor: "system",
      message: "Alert rule transitioned to active",
    },
    {
      event_time: "2026-05-18T10:00:05Z",
      event_type: "alert_fired",
      actor: "system",
      message: "High CPU Alert fired: value=95.30",
    },
  ],
};

function mockIncident(
  detail: IncidentDetailResponse,
  topology?: Promise<TopologyResponse>
) {
  vi.mocked(useRuntime).mockReturnValue({
    incidents: { list: vi.fn(), get: vi.fn(async () => detail) },
    topology: { get: topology ? vi.fn(() => topology) : vi.fn() },
  } as unknown as RuntimeApi);
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <IncidentDetailPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("renders timeline events with humanized labels", async () => {
  mockIncident(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1, name: "CPU spike" }));
  expect(screen.getByText("triggered")).toBeInTheDocument();
  expect(screen.getByText("alert fired")).toBeInTheDocument();
});

test("renders view rule link on alert_fired when triggered_by_rule_id is set", async () => {
  mockIncident(baseDetail);
  renderPage();
  await waitFor(() => screen.getByText("→ View rule"));
  expect(screen.getByText("→ View rule")).toBeInTheDocument();
});

test("does not render view rule link when triggered_by_rule_id is null", async () => {
  mockIncident({
    ...baseDetail,
    triggered_by_rule_id: null,
  });
  renderPage();
  await waitFor(() => screen.getByText("alert fired"));
  expect(screen.queryByText("→ View rule")).not.toBeInTheDocument();
});

test("renders runbook_url as link when present", async () => {
  mockIncident({
    ...baseDetail,
    runbook_url: "https://runbooks.example.com/cpu-high",
  });
  renderPage();
  await waitFor(() => screen.getByText("https://runbooks.example.com/cpu-high"));
  const link = screen.getByRole("link", { name: "https://runbooks.example.com/cpu-high" });
  expect(link).toHaveAttribute("href", "https://runbooks.example.com/cpu-high");
});

test("does not render Runbook section when runbook_url is null", async () => {
  mockIncident(baseDetail);
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1 }));
  expect(screen.queryByText("Runbook")).not.toBeInTheDocument();
});

test("renders Impacted Services panel when impacted_service is set", async () => {
  mockIncident(
    {
      ...baseDetail,
      impacted_service: "payments",
    },
    Promise.resolve({
      edges: [
        {
          caller: "api-gateway",
          callee: "payments",
          request_count: 500,
          error_rate: 0.1,
          p95_latency_ms: 120,
        },
      ],
    })
  );
  renderPage();
  await waitFor(() => screen.getByText("Impacted Services"));
  expect(screen.getByText("Impacted Services")).toBeInTheDocument();
  expect(screen.getByText("→ Service Detail")).toBeInTheDocument();
});

test("does not render Impacted Services panel when impacted_service is null", async () => {
  mockIncident({
    ...baseDetail,
    impacted_service: null,
  });
  renderPage();
  await waitFor(() => screen.getByRole("heading", { level: 1 }));
  expect(screen.queryByText("Impacted Services")).not.toBeInTheDocument();
});

test("renders responder action buttons and allows acknowledging and resolving", async () => {
  const acknowledgeMock = vi.fn().mockResolvedValue({ ...baseDetail, status: "acknowledged" });
  const updateStatusMock = vi.fn().mockResolvedValue({ ...baseDetail, status: "investigating" });
  const resolveMock = vi.fn().mockResolvedValue({ ...baseDetail, status: "resolved" });
  const addCommentMock = vi.fn().mockResolvedValue({ ...baseDetail });

  vi.mocked(useRuntime).mockReturnValue({
    incidents: {
      list: vi.fn(),
      get: vi.fn(async () => baseDetail),
      acknowledge: acknowledgeMock,
      updateStatus: updateStatusMock,
      resolve: resolveMock,
      addComment: addCommentMock,
      linkDeployment: vi.fn(),
    },
    topology: { get: vi.fn() },
  } as unknown as RuntimeApi);

  renderPage();
  await waitFor(() => screen.getByRole("button", { name: "Acknowledge" }));

  const ackBtn = screen.getByRole("button", { name: "Acknowledge" });
  expect(ackBtn).toBeInTheDocument();
  const investBtn = screen.getByRole("button", { name: "Investigate" });
  expect(investBtn).toBeInTheDocument();
  const resolveBtn = screen.getByRole("button", { name: "Resolve" });
  expect(resolveBtn).toBeInTheDocument();

  fireEvent.click(ackBtn);
  await waitFor(() => expect(acknowledgeMock).toHaveBeenCalledWith("test-tenant", "inc-1"));

  fireEvent.click(investBtn);
  await waitFor(() =>
    expect(updateStatusMock).toHaveBeenCalledWith("test-tenant", "inc-1", "investigating")
  );

  fireEvent.click(resolveBtn);
  expect(resolveMock).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm resolve" }));
  await waitFor(() => expect(resolveMock).toHaveBeenCalledWith("test-tenant", "inc-1"));
});
