import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import * as alertsApi from "../../api/alerts";
import * as slosApi from "../../api/slos";
import * as notificationsApi from "../../api/notifications";
import { AlertsPage } from "./AlertsPage";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AlertsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(slosApi, "listSlos").mockResolvedValue({ items: [] });
  vi.spyOn(notificationsApi, "listNotificationChannels").mockResolvedValue([]);
});

test("renders a no_data rule with a deadman condition label", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({
    items: [
      {
        rule_id: "rule-1",
        name: "Checkout silent",
        metric_name: "checkout",
        operator: "no_data" as alertsApi.AlertRuleItem["operator"],
        threshold: 300,
        severity: "warning",
        silenced: false,
        state: "ok",
        firing: false,
        last_fired_at: undefined,
        notification_channels: [],
        auto_trigger_incident: true,
      },
    ],
  });

  renderPage();

  await waitFor(() =>
    expect(screen.getByText("No data for 300s from checkout")).toBeInTheDocument(),
  );
});

test("submitting the No data form sends a deadman create request", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
  const createSpy = vi
    .spyOn(alertsApi, "createAlertRule")
    .mockResolvedValue({
      rule_id: "rule-2",
      name: "Checkout silent",
      metric_name: "checkout",
      operator: "no_data" as alertsApi.AlertRuleItem["operator"],
      threshold: 300,
      severity: "warning",
      silenced: false,
      state: "ok",
      firing: false,
      last_fired_at: undefined,
      notification_channels: [],
      auto_trigger_incident: true,
    });

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));

  fireEvent.change(screen.getByLabelText("Alert type"), { target: { value: "deadman" } });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Checkout silent" },
  });
  fireEvent.change(screen.getByLabelText("Service name"), {
    target: { value: "checkout" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "300" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => expect(createSpy).toHaveBeenCalledTimes(1));
  expect(createSpy).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      name: "Checkout silent",
      alert_type: "deadman",
      service_name: "checkout",
      window_secs: 300,
    }),
  );
});

test("No data form rejects a blank service name", async () => {
  vi.spyOn(alertsApi, "listAlertRules").mockResolvedValue({ items: [] });
  const createSpy = vi.spyOn(alertsApi, "createAlertRule");

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));
  fireEvent.change(screen.getByLabelText("Alert type"), { target: { value: "deadman" } });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Checkout silent" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "300" },
  });

  // The Service name field carries an HTML `required` attribute, so a plain
  // click on the submit button is intercepted by jsdom's native constraint
  // validation before React's onSubmit handler ever runs. Dispatching the
  // submit event directly exercises the form's own validation logic
  // (handleCreateSubmit's "Service name is required" check) instead.
  const form = screen.getByRole("form", { name: "Create alert rule" });
  fireEvent.submit(form);

  await waitFor(() =>
    expect(screen.getByText("Service name is required")).toBeInTheDocument(),
  );
  expect(createSpy).not.toHaveBeenCalled();
});
