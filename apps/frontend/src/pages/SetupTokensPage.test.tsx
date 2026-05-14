import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, test, expect, afterEach } from "vitest";
import SetupTokensPage from "./SetupTokensPage";
import { TenantContextProvider } from "../hooks/useTenantContext";

// ── Mocks ─────────────────────────────────────────────────────────────────────

vi.mock("../api/tokens", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/tokens")>();
  return {
    ...actual,
    listTokens: vi.fn().mockResolvedValue({ tokens: [] }),
    createToken: vi.fn(),
    revokeToken: vi.fn(),
    renewToken: vi.fn(),
    restoreToken: vi.fn(),
    deleteToken: vi.fn(),
  };
});

import { listTokens, createToken } from "../api/tokens";
const mockListTokens = vi.mocked(listTokens);
const mockCreateToken = vi.mocked(createToken);

const DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <TenantContextProvider>
      <QueryClientProvider client={qc}>
        <SetupTokensPage />
      </QueryClientProvider>
    </TenantContextProvider>
  );
}

afterEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("SetupTokensPage", () => {
  test("renders ingestion tokens heading", async () => {
    renderPage();
    expect(await screen.findByRole("heading", { name: "Ingestion tokens", level: 1 })).toBeInTheDocument();
    expect(await screen.findByText("No tokens registered.")).toBeInTheDocument();
  });

  test("shows token list when tokens exist", async () => {
    mockListTokens.mockResolvedValue({
      tokens: [
        {
          id: "tok-1",
          name: "shop-api",
          tenant_name: "Default",
          environment: "staging",
          created_at: "2026-05-01T00:00:00Z",
          revoked: false,
        },
      ],
    });
    renderPage();
    expect(await screen.findByText("shop-api")).toBeInTheDocument();
    expect(screen.getByText("staging")).toBeInTheDocument();
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  test("can create a new token", async () => {
    mockListTokens.mockResolvedValue({ tokens: [] });
    mockCreateToken.mockResolvedValue({
      id: "tok-new",
      name: "api-prod",
      tenant_name: "Default",
      environment: "production",
      created_at: "2026-05-14T00:00:00Z",
      revoked: false,
      plaintext: "secret-token-123",
    });
    renderPage();
    await screen.findByText("No tokens registered.");

    fireEvent.click(screen.getByText("+ New token"));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "api-prod" } });
    fireEvent.change(screen.getByLabelText("Environment"), { target: { value: "production" } });
    fireEvent.click(screen.getByText("Create token"));

    await waitFor(() =>
      expect(mockCreateToken).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        { name: "api-prod", environment: "production" }
      )
    );
  });

  test("shows validation error when name or environment is empty", async () => {
    mockListTokens.mockResolvedValue({ tokens: [] });
    renderPage();
    await screen.findByText("No tokens registered.");

    fireEvent.click(screen.getByText("+ New token"));
    fireEvent.click(screen.getByText("Create token"));

    expect(await screen.findByText("Name and environment are required.")).toBeInTheDocument();
    expect(mockCreateToken).not.toHaveBeenCalled();
  });
});
