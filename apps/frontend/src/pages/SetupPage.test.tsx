import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, test, expect, afterEach } from "vitest";
import SetupPage from "./SetupPage";

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
    getConfig: vi.fn(),
    saveLlmKey: vi.fn(),
  };
});

import { getConfig, saveLlmKey } from "../api/setup";
const mockGetConfig = vi.mocked(getConfig);
const mockSaveLlmKey = vi.mocked(saveLlmKey);

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <SetupPage />
    </QueryClientProvider>
  );
}

afterEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("SetupPage — AI/NLQ panel", () => {
  test("shows Not configured badge when llm_key_configured is false", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    renderPage();
    expect(await screen.findByText("Not configured")).toBeInTheDocument();
  });

  test("shows Configured badge when llm_key_configured is true", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: true });
    renderPage();
    expect(await screen.findByText("Configured")).toBeInTheDocument();
  });

  test("Save button is disabled when input is empty", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    renderPage();
    await screen.findByTestId("llm-key-input");
    expect(screen.getByTestId("llm-key-save")).toBeDisabled();
  });

  test("Save button enables when key is entered", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.change(screen.getByTestId("llm-key-input"), {
      target: { value: "sk-test-key" },
    });
    expect(screen.getByTestId("llm-key-save")).not.toBeDisabled();
  });

  test("calls saveLlmKey with trimmed input on submit", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    mockSaveLlmKey.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.change(screen.getByTestId("llm-key-input"), {
      target: { value: "  sk-test-key  " },
    });
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() => expect(mockSaveLlmKey).toHaveBeenCalledWith("sk-test-key"));
  });

  test("shows success message after save", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    mockSaveLlmKey.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.change(screen.getByTestId("llm-key-input"), {
      target: { value: "sk-abc" },
    });
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(screen.getByTestId("llm-key-saved")).toBeInTheDocument()
    );
  });

  test("shows error message when saveLlmKey fails", async () => {
    mockGetConfig.mockResolvedValue({ llm_key_configured: false });
    mockSaveLlmKey.mockRejectedValue(new Error("server error"));
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.change(screen.getByTestId("llm-key-input"), {
      target: { value: "sk-bad" },
    });
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(screen.getByTestId("llm-key-error")).toBeInTheDocument()
    );
  });
});
