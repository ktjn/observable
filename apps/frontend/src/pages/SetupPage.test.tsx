import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, test, expect, afterEach } from "vitest";
import SetupPage from "./SetupPage";
import { TenantContextProvider } from "../hooks/useTenantContext";

// ── Mocks ─────────────────────────────────────────────────────────────────────

vi.mock("../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/setup")>();
  return {
    ...actual,
    getFirstSignalStatus: vi.fn().mockResolvedValue({
      state: "waiting",
      traces: 0,
      logs: 0,
      metrics: 0,
    }),
  };
});

import { getFirstSignalStatus } from "../api/setup";
const mockGetFirstSignalStatus = vi.mocked(getFirstSignalStatus);

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <TenantContextProvider>
      <QueryClientProvider client={qc}>
        <SetupPage />
      </QueryClientProvider>
    </TenantContextProvider>
  );
}

afterEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("SetupPage — ingest panel", () => {
  test("renders collector endpoint URLs", () => {
    renderPage();
    expect(screen.getByText("Collector endpoint")).toBeInTheDocument();
    expect(screen.getByText("OTLP gRPC Ingestion URL")).toBeInTheDocument();
    expect(screen.getByText("OTLP HTTP/JSON Traces")).toBeInTheDocument();
    expect(screen.getByText("OTLP HTTP/JSON Metrics")).toBeInTheDocument();
    expect(screen.getByText("OTLP HTTP/JSON Logs")).toBeInTheDocument();
  });

  test("shows waiting status by default", async () => {
    renderPage();
    expect(await screen.findByText("Waiting for first signal")).toBeInTheDocument();
  });

  test("shows detected status when signals are present", async () => {
    mockGetFirstSignalStatus.mockResolvedValue({
      state: "detected",
      traces: 5,
      logs: 12,
      metrics: 3,
    });
    renderPage();
    expect(await screen.findByText("First signal detected")).toBeInTheDocument();
    expect(screen.getByText("5")).toBeInTheDocument();
    expect(screen.getByText("12")).toBeInTheDocument();
    expect(screen.getByText("3")).toBeInTheDocument();
  });

  test("shows error status when check returns error state", async () => {
    mockGetFirstSignalStatus.mockResolvedValue({
      state: "error",
      traces: 0,
      logs: 0,
      metrics: 0,
    });
    renderPage();
    expect(await screen.findByText("First signal check failed")).toBeInTheDocument();
  });
});
