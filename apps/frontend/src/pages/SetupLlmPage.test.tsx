import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { vi, describe, test, expect, afterEach } from "vitest";
import SetupLlmPage from "./SetupLlmPage";
import { TenantContextProvider } from "../hooks/useTenantContext";

// ── Mocks ─────────────────────────────────────────────────────────────────────

vi.mock("../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/setup")>();
  return {
    ...actual,
    getConfig: vi.fn(),
    saveLlmConfig: vi.fn(),
    fetchAvailableModels: vi.fn(),
  };
});

import { getConfig, saveLlmConfig, fetchAvailableModels } from "../api/setup";
const mockGetConfig = vi.mocked(getConfig);
const mockSaveLlmConfig = vi.mocked(saveLlmConfig);
const mockFetchAvailableModels = vi.mocked(fetchAvailableModels);

vi.mock("../lib/webllm/webllmEngine", () => ({
  listAvailableModels: vi.fn(),
  checkWebGpuSupport: vi.fn(),
}));

import { listAvailableModels, checkWebGpuSupport } from "../lib/webllm/webllmEngine";
const mockListAvailableModels = vi.mocked(listAvailableModels);
const mockCheckWebGpuSupport = vi.mocked(checkWebGpuSupport);

const BASE_CONFIG = {
  llm_key_configured: false,
  llm_url: null,
  llm_model: null,
  llm_provider: "remote" as const,
  webllm_model: null,
};

const DEFAULT_TENANT_ID = "00000000-0000-0000-0000-000000000001";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <TenantContextProvider>
      <QueryClientProvider client={qc}>
        <SetupLlmPage />
      </QueryClientProvider>
    </TenantContextProvider>
  );
}

afterEach(() => {
  vi.clearAllMocks();
});

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("SetupLlmPage", () => {
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

  test("Save button is enabled with empty API key (url/model may be saved)", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    renderPage();
    await screen.findByTestId("llm-key-input");
    // Wait for config to load — button is disabled until the query resolves.
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-save")).not.toBeDisabled()
    );
  });

  test("url and model inputs pre-fill from server config on load", async () => {
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
      // No remote models loaded yet → text input shown
      expect((screen.getByTestId("llm-model-input") as HTMLInputElement).value).toBe("microsoft/Phi-3-mini-4k-instruct");
    });
  });

  test("pre-filled url can be cleared by the user", async () => {
    mockGetConfig.mockResolvedValue({
      ...BASE_CONFIG,
      llm_url: "http://gpu-host:8000",
      llm_model: null,
    });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    await waitFor(() => {
      expect((urlInput as HTMLInputElement).value).toBe("http://gpu-host:8000");
    });
    // User clears the field — it should stay empty, not revert to config value.
    fireEvent.change(urlInput, { target: { value: "" } });
    expect((urlInput as HTMLInputElement).value).toBe("");
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
        DEFAULT_TENANT_ID,
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
    // No remote models loaded → text input shown for model
    fireEvent.change(screen.getByTestId("llm-model-input"), {
      target: { value: "gpt-4o" },
    });
    fireEvent.submit(screen.getByTestId("llm-url-input").closest("form")!);
    await waitFor(() =>
      expect(mockSaveLlmConfig).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ url: "http://localhost:8000", model: "gpt-4o" })
      )
    );
  });

  test("shows Saved badge after successful save (no auto-test)", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-key-input");
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-saved")).toBeInTheDocument()
    );
    expect(screen.getByTestId("llm-config-saved").textContent).toContain("Saved");
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
    fireEvent.submit(screen.getByTestId("llm-key-input").closest("form")!);
    await waitFor(() => expect(mockSaveLlmConfig).toHaveBeenCalled());
    const callArg = mockSaveLlmConfig.mock.calls[0][1];
    expect(callArg).not.toHaveProperty("apiKey");
  });

  test("Test connection button is visible when URL field is non-empty", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    // Not visible before URL is entered
    expect(screen.queryByTestId("llm-config-test")).not.toBeInTheDocument();
    // Enter a URL — button should appear
    fireEvent.change(urlInput, { target: { value: "http://192.168.0.234:11434/v1" } });
    expect(screen.getByTestId("llm-config-test")).toBeInTheDocument();
  });

  test("Test connection button calls fetchAvailableModels with form URL and key", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockFetchAvailableModels.mockResolvedValue({ ok: true, models: [] });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    fireEvent.change(urlInput, { target: { value: "http://ollama:11434/v1" } });
    fireEvent.click(screen.getByTestId("llm-config-test"));
    await waitFor(() =>
      expect(mockFetchAvailableModels).toHaveBeenCalledWith(DEFAULT_TENANT_ID, "http://ollama:11434/v1", undefined)
    );
  });

  test("model dropdown appears with fetched options when Test connection succeeds", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockFetchAvailableModels.mockResolvedValue({
      ok: true,
      models: ["llama3.1:8b", "phi3.5:latest", "phi3:latest"],
    });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    fireEvent.change(urlInput, { target: { value: "http://ollama:11434/v1" } });
    fireEvent.click(screen.getByTestId("llm-config-test"));

    await waitFor(() =>
      expect(screen.getByTestId("llm-model-select")).toBeInTheDocument()
    );
    expect(screen.getByRole("option", { name: "llama3.1:8b" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "phi3.5:latest" })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "phi3:latest" })).toBeInTheDocument();
  });

  test("connected badge shown after successful Test connection", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockFetchAvailableModels.mockResolvedValue({
      ok: true,
      models: ["llama3.1:8b"],
    });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    fireEvent.change(urlInput, { target: { value: "http://ollama:11434/v1" } });
    fireEvent.click(screen.getByTestId("llm-config-test"));
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-test-ok")).toBeInTheDocument()
    );
    expect(screen.getByTestId("llm-config-test-ok").textContent).toContain("Connected");
  });

  test("text input shown as fallback when Test connection returns empty model list", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockFetchAvailableModels.mockResolvedValue({ ok: true, models: [] });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    fireEvent.change(urlInput, { target: { value: "http://openai-compat:8000/v1" } });
    fireEvent.click(screen.getByTestId("llm-config-test"));
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-test-ok")).toBeInTheDocument()
    );
    // No models returned → text input fallback remains
    expect(screen.queryByTestId("llm-model-select")).not.toBeInTheDocument();
    expect(screen.getByTestId("llm-model-input")).toBeInTheDocument();
  });

  test("error badge shown when Test connection returns ok: false", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockFetchAvailableModels.mockResolvedValue({
      ok: false,
      models: [],
      error: "connection refused",
    });
    renderPage();
    const urlInput = await screen.findByTestId("llm-url-input");
    fireEvent.change(urlInput, { target: { value: "http://bad-host:1/v1" } });
    fireEvent.click(screen.getByTestId("llm-config-test"));
    await waitFor(() =>
      expect(screen.getByTestId("llm-config-test-failed")).toBeInTheDocument()
    );
    expect(screen.getByTestId("llm-config-test-failed").textContent).toContain("connection refused");
  });

  // ── Provider selector / WebLLM branch ────────────────────────────────────────

  test("defaults to Remote provider and shows the remote fields", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    renderPage();
    await screen.findByTestId("llm-provider-select");
    expect((screen.getByTestId("llm-provider-select") as HTMLSelectElement).value).toBe("remote");
    expect(screen.getByTestId("llm-key-input")).toBeInTheDocument();
    expect(screen.getByTestId("llm-url-input")).toBeInTheDocument();
    expect(screen.queryByTestId("webllm-model-select")).not.toBeInTheDocument();
    expect(mockListAvailableModels).not.toHaveBeenCalled();
    expect(mockCheckWebGpuSupport).not.toHaveBeenCalled();
  });

  test("pre-fills provider select from config.llm_provider", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG, llm_provider: "webllm", webllm_model: "Llama-3-8B" });
    mockListAvailableModels.mockResolvedValue([{ modelId: "Llama-3-8B", label: "Llama-3-8B" }]);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    renderPage();
    await waitFor(() =>
      expect((screen.getByTestId("llm-provider-select") as HTMLSelectElement).value).toBe("webllm")
    );
  });

  test("switching provider to WebLLM hides remote fields and shows model picker + GPU badge", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockListAvailableModels.mockResolvedValue([
      { modelId: "Llama-3-8B-Instruct-q4f16_1", label: "Llama-3-8B-Instruct-q4f16_1" },
    ]);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    renderPage();
    await screen.findByTestId("llm-provider-select");
    fireEvent.change(screen.getByTestId("llm-provider-select"), { target: { value: "webllm" } });

    expect(screen.queryByTestId("llm-key-input")).not.toBeInTheDocument();
    expect(screen.queryByTestId("llm-url-input")).not.toBeInTheDocument();

    await waitFor(() => expect(mockListAvailableModels).toHaveBeenCalled());
    await waitFor(() => expect(mockCheckWebGpuSupport).toHaveBeenCalled());
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "Llama-3-8B-Instruct-q4f16_1" })).toBeInTheDocument()
    );
    await waitFor(() =>
      expect(screen.getByTestId("webllm-gpu-badge").textContent).toContain("WebGPU supported")
    );
  });

  test("WebGPU unsupported shows a warn badge with the reason", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockListAvailableModels.mockResolvedValue([]);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: false, reason: "No compatible GPU adapter found" });
    renderPage();
    await screen.findByTestId("llm-provider-select");
    fireEvent.change(screen.getByTestId("llm-provider-select"), { target: { value: "webllm" } });
    await waitFor(() =>
      expect(screen.getByTestId("webllm-gpu-badge").textContent).toContain("No compatible GPU adapter found")
    );
  });

  test("saving in Remote mode calls saveLlmConfig with the existing shape (regression)", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    renderPage();
    await screen.findByTestId("llm-url-input");
    fireEvent.change(screen.getByTestId("llm-url-input"), { target: { value: "http://localhost:8000" } });
    fireEvent.change(screen.getByTestId("llm-model-input"), { target: { value: "gpt-4o" } });
    fireEvent.submit(screen.getByTestId("llm-url-input").closest("form")!);
    await waitFor(() =>
      expect(mockSaveLlmConfig).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ url: "http://localhost:8000", model: "gpt-4o", provider: "remote" })
      )
    );
    const callArg = mockSaveLlmConfig.mock.calls[0][1];
    expect(callArg).not.toHaveProperty("webllmModel");
  });

  test("saving in WebLLM mode submits provider + webllmModel and no stale remote fields", async () => {
    mockGetConfig.mockResolvedValue({ ...BASE_CONFIG, llm_url: "http://old-host:8000", llm_model: "gpt-4o" });
    mockSaveLlmConfig.mockResolvedValue(undefined);
    mockListAvailableModels.mockResolvedValue([
      { modelId: "Llama-3-8B-Instruct-q4f16_1", label: "Llama-3-8B-Instruct-q4f16_1" },
    ]);
    mockCheckWebGpuSupport.mockResolvedValue({ supported: true });
    renderPage();
    await screen.findByTestId("llm-provider-select");
    fireEvent.change(screen.getByTestId("llm-provider-select"), { target: { value: "webllm" } });
    await waitFor(() =>
      expect(screen.getByRole("option", { name: "Llama-3-8B-Instruct-q4f16_1" })).toBeInTheDocument()
    );
    fireEvent.change(screen.getByTestId("webllm-model-select"), {
      target: { value: "Llama-3-8B-Instruct-q4f16_1" },
    });
    fireEvent.submit(screen.getByTestId("webllm-model-select").closest("form")!);
    await waitFor(() =>
      expect(mockSaveLlmConfig).toHaveBeenCalledWith(
        DEFAULT_TENANT_ID,
        expect.objectContaining({ provider: "webllm", webllmModel: "Llama-3-8B-Instruct-q4f16_1" })
      )
    );
    const callArg = mockSaveLlmConfig.mock.calls[0][1];
    expect(callArg).not.toHaveProperty("apiKey");
    expect(callArg).not.toHaveProperty("url");
    expect(callArg).not.toHaveProperty("model");
  });
});
