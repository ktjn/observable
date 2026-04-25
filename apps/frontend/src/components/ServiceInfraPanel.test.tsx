import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, it, expect, beforeEach } from "vitest";
import { ServiceInfraPanel } from "./ServiceInfraPanel";
import * as infraApi from "../api/infrastructure";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

describe("ServiceInfraPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders entity cards with links", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({
      items: [
        {
          entity_type: "pod",
          entity_id: "checkout-pod-1",
          display_name: "checkout-pod-1",
          parent_id: null,
          parent_display_name: null,
          environment: "prod",
          health_state: "healthy",
          last_seen_unix_nano: 0,
          related_services: ["checkout"],
          log_rate_per_minute: null,
          error_rate: null,
          restart_count: null,
          cpu_usage: 0.42,
          memory_usage: 0.31,
          disk_usage: null,
          network_io: null,
        },
      ],
    });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toBeInTheDocument()
    );
    expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toHaveAttribute(
      "href",
      "/infrastructure/pod/checkout-pod-1"
    );
    expect(screen.getByText("pod")).toBeInTheDocument();
  });

  it("encodes entity_id in the link href", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({
      items: [
        {
          entity_type: "pod",
          entity_id: "checkout/pod:1",
          display_name: "checkout-pod-1",
          parent_id: null,
          parent_display_name: null,
          environment: null,
          health_state: "healthy",
          last_seen_unix_nano: 0,
          related_services: [],
          log_rate_per_minute: null,
          error_rate: null,
          restart_count: null,
          cpu_usage: null,
          memory_usage: null,
          disk_usage: null,
          network_io: null,
        },
      ],
    });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toBeInTheDocument()
    );
    expect(screen.getByRole("link", { name: /checkout-pod-1/ })).toHaveAttribute(
      "href",
      "/infrastructure/pod/checkout%2Fpod%3A1"
    );
  });

  it("shows empty state when no entities", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({ items: [] });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(
        screen.getByText("No infrastructure entities observed for this service.")
      ).toBeInTheDocument()
    );
  });

  it("shows error state when fetch fails", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockRejectedValue(new Error("fail"));

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() =>
      expect(screen.getByText("Could not load infrastructure.")).toBeInTheDocument()
    );
  });

  it("shows cpu and memory when available", async () => {
    vi.spyOn(infraApi, "listInfrastructure").mockResolvedValue({
      items: [
        {
          entity_type: "host",
          entity_id: "node-3",
          display_name: "node-3",
          parent_id: null,
          parent_display_name: null,
          environment: null,
          health_state: "watch",
          last_seen_unix_nano: 0,
          related_services: [],
          log_rate_per_minute: null,
          error_rate: null,
          restart_count: null,
          cpu_usage: 0.75,
          memory_usage: 0.88,
          disk_usage: null,
          network_io: null,
        },
      ],
    });

    render(<ServiceInfraPanel serviceName="checkout" />, { wrapper });

    await waitFor(() => expect(screen.getByText(/CPU/)).toBeInTheDocument());
    expect(screen.getByText(/75%/)).toBeInTheDocument();
    expect(screen.getByText(/88%/)).toBeInTheDocument();
  });
});
