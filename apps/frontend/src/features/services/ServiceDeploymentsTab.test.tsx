import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, it, expect, beforeEach } from "vitest";
import { ServiceDeploymentsTab } from "./ServiceDeploymentsTab";
import * as deploymentsApi from "../../api/deployments";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../hooks/useGlobalDateRange", () => ({
  useGlobalDateRange: () => ({ fromMs: 1_000_000, toMs: 2_000_000 }),
}));

vi.mock("../../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "iso-local-ms" }),
}));

const sampleDeployments: deploymentsApi.DeploymentMarker[] = [
  {
    deployment_id: "dep-aabbccdd",
    tenant_id: "test-tenant",
    project_id: null,
    service_name: "checkout",
    environment: "prod",
    service_version: "v1.2.3",
    status: "success",
    started_at: "2026-05-15T10:00:00Z",
    finished_at: "2026-05-15T10:05:00Z",
    deployed_by: "alice",
    commit_sha: "abc123",
    rollback_of: null,
    metadata: null,
  },
  {
    deployment_id: "dep-11223344",
    tenant_id: "test-tenant",
    project_id: null,
    service_name: "checkout",
    environment: "staging",
    service_version: "v1.2.4",
    status: "in_progress",
    started_at: "2026-05-15T11:00:00Z",
    finished_at: null,
    deployed_by: "bob",
    commit_sha: null,
    rollback_of: null,
    metadata: null,
  },
];

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("ServiceDeploymentsTab", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders deployment rows with version and environment", async () => {
    vi.spyOn(deploymentsApi, "listDeployments").mockResolvedValue({
      items: sampleDeployments,
    });

    render(<ServiceDeploymentsTab serviceName="checkout" />, { wrapper });

    await waitFor(() => screen.getByText("v1.2.3"));
    expect(screen.getByText("v1.2.3")).toBeInTheDocument();
    expect(screen.getByText("v1.2.4")).toBeInTheDocument();
    expect(screen.getByText("prod")).toBeInTheDocument();
    expect(screen.getByText("staging")).toBeInTheDocument();
    expect(screen.getByText("alice")).toBeInTheDocument();
    expect(screen.getByText("bob")).toBeInTheDocument();
  });

  it("passes service_name filter to listDeployments", async () => {
    const spy = vi
      .spyOn(deploymentsApi, "listDeployments")
      .mockResolvedValue({ items: [] });

    render(<ServiceDeploymentsTab serviceName="checkout" />, { wrapper });

    await waitFor(() => expect(spy).toHaveBeenCalled());
    expect(spy).toHaveBeenCalledWith(
      "test-tenant",
      expect.objectContaining({ service_name: "checkout" }),
    );
  });

  it("shows success badge tone for successful deployments", async () => {
    vi.spyOn(deploymentsApi, "listDeployments").mockResolvedValue({
      items: [sampleDeployments[0]],
    });

    render(<ServiceDeploymentsTab serviceName="checkout" />, { wrapper });

    await waitFor(() => screen.getByText("success"));
    expect(screen.getByText("success")).toBeInTheDocument();
  });

  it("shows empty state when no deployments", async () => {
    vi.spyOn(deploymentsApi, "listDeployments").mockResolvedValue({ items: [] });

    render(<ServiceDeploymentsTab serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByText("No deployments found.")).toBeInTheDocument(),
    );
  });
});
