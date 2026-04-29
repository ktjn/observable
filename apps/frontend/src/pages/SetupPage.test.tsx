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
    saveLlmConfig: vi.fn(),
    saveLlmKey: vi.fn(),
  };
});

import { getConfig, saveLlmConfig } from "../api/setup";
const mockGetConfig = vi.mocked(getConfig);
const mockSaveLlmConfig = vi.mocked(saveLlmConfig);

const BASE_CONFIG = {
  llm_key_configured: false,
  llm_url: null,
  llm_model: null,
};

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
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    renderPage();
    expect(await screen.findByText("Not configured")).toBeInTheDocument();
  });

  test("shows Configured badge when llm_key_configured is true", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG, llm_key_configured: true });
    renderPage();
    expect(await screen.findByText("Configured")).toBeInTheDocument();
  });

  test("Save button is enabled even with empty API key (url/model may be saved)", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    renderPage();
    await screen.findByTestId("llm-key-input");
    expect(screen.getByTestId("llm-config-save")).not.toBeDisabled();
  });

  test("url and model inputs pre-fill from server config", async () => {
    mockGetConfig.mockResolvedValue({
      ...BASE_CONFIG,
      llm_key_configured: true,
      llm_url: "http://gpu-host:8000",
      llm_model: "microsoft/Phi-3-mini-4k-instruct",
    });
    renderPage();
    await screen.findByTestId("llm-url-input");
    await waitFor(() => {
      expect((screen.getByTestId("llm-url-input") as HTMLInputElement).value).toBe("http://gpu-host:8000");
      expect((screen.getByTestId("llm-model-input") as HTMLInputElement).value).toBe("microsoft/Phi-3-mini-4k-instruct");
    });
  });

  test("calls saveLlmConfig with apiKey when key is entered", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.change(screen.getByTestId("llm-key-input"), {
      target: { value: "  sk-test-key  " },
    });
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(mockSaveLlmConfig).toHaveBeenCalledWith(
        expect.objectContaining({ apiKey: "sk-test-key" })
      )
    );
  });

  test("calls saveLlmConfig with url and model when provided", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-url-input");
    fireEvent.change(screen.getByTestId("llm-url-input"), {
      target: { value: "http://localhost:8000" },
    });
    fireEvent.change(screen.getByTestId("llm-model-input"), {
      target: { value: "gpt-4o" },
    });
    fireEvent.submit(screen.getByTestId("llm-url-input").closest("form")!);
    await waitFor(() =>
      expect(mockSaveLlmConfig).toHaveBeenCalledWith(
        expect.objectContaining({ url: "http://localhost:8000", model: "gpt-4o" })
      )
    );
  });

  test("shows success message after save", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-saved")).toBeInTheDocument()
    );
  });

  test("shows error message when saveLlmConfig fails", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockRejectedValue(new Error("server error"));
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-error")).toBeInTheDocument()
    );
  });

  test("does not include apiKey in saveLlmConfig when key input is empty", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    // Ensure key input is empty, then submit.
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() => expect(mockSaveLlmConfig).toHaveBeenCalled());
    const callArg = mockSaveLlmConfig.mock.calls[0][0];
    expect(callArg).not.toHaveProperty("apiKey");
  });
});
