import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, test, vi } from "vitest";
import type { NotificationChannelItem } from "../../api/notifications";
import { NotificationChannelsList } from "./NotificationChannelsList";
import type { RuntimeApi } from "../../runtime/types";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

import { useRuntime } from "../../hooks/useRuntime";

const listMock = vi.fn<() => Promise<NotificationChannelItem[]>>();

function mockChannels() {
  vi.mocked(useRuntime).mockReturnValue({
    notificationChannels: {
      list: listMock,
      create: vi.fn(),
      delete: vi.fn(),
    },
  } as unknown as RuntimeApi);
}

function renderList() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <NotificationChannelsList />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("renders EmptyState when there are no channels", async () => {
  listMock.mockResolvedValue([]);
  mockChannels();

  renderList();

  expect(await screen.findByText("No notification channels")).toBeInTheDocument();
});

test("renders channel list when channels are present", async () => {
  listMock.mockResolvedValue([
    {
      channel_id: "chan-1",
      name: "Prod Webhook",
      channel_type: "webhook",
      config: { url: "https://example.com/hook" },
    } as unknown as NotificationChannelItem,
  ]);
  mockChannels();

  renderList();

  expect(await screen.findByText("Prod Webhook")).toBeInTheDocument();
});

test("renders ErrorState with retry when channels fail to load", async () => {
  listMock
    .mockRejectedValueOnce(new Error("network error"))
    .mockResolvedValueOnce([]);
  mockChannels();

  renderList();

  expect(await screen.findByText("Failed to load channels")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  await waitFor(() => expect(listMock).toHaveBeenCalledTimes(2));
  expect(await screen.findByText("No notification channels")).toBeInTheDocument();
});
