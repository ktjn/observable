import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test, vi } from "vitest";
import { SavedViewsControl } from "./SavedViewsControl";
import type { LogViewConfig, SavedView } from "../../../api/savedViews";

const baseConfig: LogViewConfig = {
  query: null,
  severity_filter: "all",
  message_search: "",
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

vi.mock("../../../api/savedViews", async () => {
  const actual = await vi.importActual<typeof import("../../../api/savedViews")>("../../../api/savedViews");
  return {
    ...actual,
    fetchSavedViews: vi.fn(async () => ({ items: [savedView] })),
    createSavedView: vi.fn(async () => savedView),
    deleteSavedView: vi.fn(async () => undefined),
    updateSavedView: vi.fn(async () => ({ ...savedView, visibility: "public" })),
    fetchSavedViewGrants: vi.fn(async () => ({ grants: [] })),
    addSavedViewGrant: vi.fn(async () => undefined),
    revokeSavedViewGrant: vi.fn(async () => undefined),
  };
});

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

test("loading a saved view calls onLoad with its config", async () => {
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

test("saving the current view calls createSavedView with the current config", async () => {
  const { createSavedView } = await import("../../../api/savedViews");
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
    expect(createSavedView).toHaveBeenCalledWith("tenant-1", {
      name: "My new view",
      signal_kind: "logs",
      config: baseConfig,
    }),
  );
});

test("toggling visibility calls updateSavedView with the flipped value", async () => {
  const { updateSavedView } = await import("../../../api/savedViews");
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByRole("button", { name: /make public/i }));
  fireEvent.click(screen.getByRole("button", { name: /make public/i }));

  await waitFor(() =>
    expect(updateSavedView).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, {
      name: savedView.name,
      config: savedView.config,
      visibility: "public",
    }),
  );
});

test("adding a grant calls addSavedViewGrant with the entered user id and relation", async () => {
  const { addSavedViewGrant } = await import("../../../api/savedViews");
  render(<SavedViewsControl tenantId="tenant-1" currentConfig={baseConfig} onLoad={vi.fn()} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /saved views/i }));
  await waitFor(() => screen.getByText("Errors in checkout"));
  fireEvent.click(screen.getByRole("button", { name: /manage errors in checkout/i }));
  await waitFor(() => screen.getByPlaceholderText("User ID"));
  fireEvent.change(screen.getByPlaceholderText("User ID"), { target: { value: "user-42" } });
  fireEvent.click(screen.getByRole("button", { name: /^add$/i }));

  await waitFor(() =>
    expect(addSavedViewGrant).toHaveBeenCalledWith("tenant-1", savedView.saved_view_id, "user-42", "viewer"),
  );
});
