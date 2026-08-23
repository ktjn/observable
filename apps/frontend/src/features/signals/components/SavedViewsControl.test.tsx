import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test, vi } from "vitest";
import { SavedViewsControl } from "./SavedViewsControl";
import type { LogViewConfig, SavedView } from "../../../api/savedViews";
import type { RuntimeApi } from "../../../runtime/types";

const baseConfig: LogViewConfig = {
  query: null,
  severity_filter: "all",
  time_range: { mode: "preset", preset: "1h" },
  visible_columns: ["level", "service"],
};

const savedView: SavedView = {
  saved_view_id: "view-1",
  name: "Errors in checkout",
  signal_kind: "logs",
  visibility: "private",
  config: { ...baseConfig, severity_filter: "error" },
  created_at: "2026-07-01T00:00:00Z",
  updated_at: "2026-07-01T00:00:00Z",
};

vi.mock("../../../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

import { useRuntime } from "../../../hooks/useRuntime";

const listMock = vi.fn(async () => ({ items: [savedView] }));
const createMock = vi.fn(async () => savedView);
const updateMock = vi.fn(async () => ({ ...savedView, visibility: "public" as const }));
const deleteMock = vi.fn(async () => undefined);
const grantsMock = vi.fn(async () => ({ grants: [] }));
const addGrantMock = vi.fn(async () => undefined);
const revokeGrantMock = vi.fn(async () => undefined);

function mockRuntime() {
  vi.mocked(useRuntime).mockReturnValue({
    savedViews: {
      list: listMock,
      create: createMock,
      update: updateMock,
      delete: deleteMock,
      listGrants: grantsMock,
      addGrant: addGrantMock,
      revokeGrant: revokeGrantMock,
    },
  } as unknown as RuntimeApi);
}

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

test("loading a saved view calls onLoad with its config", async () => {
  mockRuntime();
  const onLoad = vi.fn();
  render(
    <SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={onLoad} />,
    { wrapper },
  );

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByText("Errors in checkout"));

  expect(onLoad).toHaveBeenCalledWith(savedView.config);
});

test("saving the current view calls savedViews.create with the current config", async () => {
  mockRuntime();
  render(
    <SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />,
    { wrapper },
  );

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /save current view/i }));
  fireEvent.change(screen.getByLabelText(/view name/i), { target: { value: "My new view" } });
  fireEvent.click(screen.getByRole("button", { name: /^save$/i }));

  await waitFor(() =>
    expect(createMock).toHaveBeenCalledWith("tenant-1", {
      name: "My new view",
      signal_kind: "logs",
      config: baseConfig,
    }),
  );
});

test("toggling visibility calls savedViews.update with the flipped value", async () => {
  mockRuntime();
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByRole("button", { name: /make public/i }));
  fireEvent.click(screen.getByRole("button", { name: /make public/i }));

  await waitFor(() =>
    expect(updateMock).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, {
      name: savedView.name,
      config: savedView.config,
      visibility: "public",
    }),
  );
});

test("adding a grant calls savedViews.addGrant with the entered user id and relation", async () => {
  mockRuntime();
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByPlaceholderText("User ID"));
  fireEvent.change(screen.getByPlaceholderText("User ID"), { target: { value: "user-42" } });
  fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

  await waitFor(() =>
    expect(addGrantMock).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, "user-42", "viewer"),
  );
});
