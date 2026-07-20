import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, expect, test, vi } from "vitest";
import * as notificationsApi from "../../api/notifications";
import { NotificationChannelsList } from "./NotificationChannelsList";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

function renderList() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <NotificationChannelsList />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
});

test("renders EmptyState when there are no channels", async () => {
  vi.spyOn(notificationsApi, "listNotificationChannels").mockResolvedValue([]);

  renderList();

  expect(await screen.findByText("No notification channels")).toBeInTheDocument();
});

test("renders channel list when channels are present", async () => {
  vi.spyOn(notificationsApi, "listNotificationChannels").mockResolvedValue([
    {
      channel_id: "chan-1",
      name: "Prod Webhook",
      channel_type: "webhook",
      config: { url: "https://example.com/hook" },
    } as unknown as notificationsApi.NotificationChannelItem,
  ]);

  renderList();

  expect(await screen.findByText("Prod Webhook")).toBeInTheDocument();
});

test("renders ErrorState with retry when channels fail to load", async () => {
  const listSpy = vi
    .spyOn(notificationsApi, "listNotificationChannels")
    .mockRejectedValueOnce(new Error("network error"))
    .mockResolvedValueOnce([]);

  renderList();

  expect(await screen.findByText("Failed to load channels")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  await waitFor(() => expect(listSpy).toHaveBeenCalledTimes(2));
  expect(await screen.findByText("No notification channels")).toBeInTheDocument();
});
