import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import QueryWorkbenchPage from "./QueryWorkbenchPage";
import { encodeWorkbenchState } from "../features/workbench/workbenchState";
import { TenantContextProvider } from "../hooks/useTenantContext";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    useSearch: vi.fn(() => ({})),
    useNavigate: () => vi.fn(),
  };
});

vi.mock("../features/workbench/NotebookEditor", () => ({
  NotebookEditor: () => <textarea aria-label="Notebook editor" />,
}));

vi.mock("../api/setup", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api/setup")>();
  return {
    ...actual,
    getConfig: vi.fn().mockResolvedValue({
      llm_key_configured: true,
      llm_url: null,
      llm_model: null,
      llm_provider: "remote" as const,
      webllm_model: null,
    }),
  };
});

import { useSearch } from "@tanstack/react-router";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <TenantContextProvider>
      <QueryClientProvider client={qc}>{children}</QueryClientProvider>
    </TenantContextProvider>
  );
}

describe("QueryWorkbenchPage", () => {
  test("renders the starter notebook title when no URL state is present", () => {
    vi.mocked(useSearch).mockReturnValue({} as never);

    render(<QueryWorkbenchPage />, { wrapper });

    expect(screen.getByRole("heading", { name: "Query Workbench" })).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-metrics")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-logs")).toBeInTheDocument();
    expect(screen.getByTestId("workbench-block-traces")).toBeInTheDocument();
  });

  test("restores the notebook title from the encoded URL state", () => {
    const encoded = encodeWorkbenchState({
      version: 1,
      title: "Payments Notebook",
      activeBlockId: "metrics",
      blocks: [
        { id: "metrics", signal: "metrics", mode: "nlq", draft: "", collapsed: false },
        { id: "logs", signal: "logs", mode: "nlq", draft: "", collapsed: false },
        { id: "traces", signal: "traces", mode: "nlq", draft: "", collapsed: false },
      ],
    });
    vi.mocked(useSearch).mockReturnValue({ state: encoded } as never);

    render(<QueryWorkbenchPage />, { wrapper });

    expect(screen.getByRole("heading", { name: "Payments Notebook" })).toBeInTheDocument();
  });
});
