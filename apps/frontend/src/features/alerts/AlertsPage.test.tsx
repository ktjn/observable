import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, test, vi, beforeEach } from "vitest";
import type { AlertRuleItem, AlertRuleListResponse } from "../../api/alerts";
import { AlertsPage } from "./AlertsPage";
import type { RuntimeApi } from "../../runtime/types";

vi.mock("../../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

vi.mock("../../hooks/useRuntime", () => ({
  useRuntime: vi.fn(),
}));

import { useRuntime } from "../../hooks/useRuntime";

const createMock = vi.fn<(req: unknown) => Promise<{ rule_id: string }>>();

function mockRuntime(rules: AlertRuleItem[]) {
  vi.mocked(useRuntime).mockReturnValue({
    alerts: {
      list: vi.fn(async (): Promise<AlertRuleListResponse> => ({ items: rules })),
      create: createMock,
    },
    slos: {
      list: vi.fn(async () => ({ items: [] })),
      create: vi.fn(),
    },
    notificationChannels: {
      list: vi.fn(async () => []),
      create: vi.fn(),
      delete: vi.fn(),
    },
  } as unknown as RuntimeApi);
}

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <AlertsPage />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

test("renders a no_data rule with a deadman condition label", async () => {
  mockRuntime([
    {
      rule_id: "rule-1",
      name: "Checkout silent",
      metric_name: "checkout",
      operator: "no_data" as AlertRuleItem["operator"],
      threshold: 300,
      severity: "warning",
      silenced: false,
      state: "ok",
      firing: false,
      last_fired_at: undefined,
      notification_channels: [],
      auto_trigger_incident: true,
      suppressed: false,
    },
  ]);

  renderPage();

  await waitFor(() =>
    expect(screen.getByText("No data for 300s from checkout")).toBeInTheDocument(),
  );
});

test("submitting the No data form sends a deadman create request", async () => {
  mockRuntime([]);
  createMock.mockResolvedValue({ rule_id: "rule-2" });

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));

  fireEvent.change(screen.getByLabelText("Alert type"), { target: { value: "deadman" } });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Checkout silent" },
  });
  fireEvent.change(screen.getByLabelText(/service name/i), {
    target: { value: "checkout" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "300" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => expect(createMock).toHaveBeenCalledTimes(1));
  expect(createMock).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      name: "Checkout silent",
      alert_type: "deadman",
      service_name: "checkout",
      window_secs: 300,
    }),
  );
});

test("No data form with blank service name succeeds and passes empty string to the create op", async () => {
  mockRuntime([]);
  createMock.mockResolvedValue({ rule_id: "rule-blank-svc" });

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
  // Service name intentionally left blank — it is now optional.

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => expect(createMock).toHaveBeenCalledTimes(1));
  expect(createMock).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      name: "Checkout silent",
      alert_type: "deadman",
      service_name: "",
      window_secs: 300,
    }),
  );
});

test("submitting the Change detection form sends a change_detection create request", async () => {
  mockRuntime([]);
  createMock.mockResolvedValue({ rule_id: "rule-3" });

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));

  fireEvent.change(screen.getByLabelText("Alert type"), {
    target: { value: "change_detection" },
  });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Error rate shift" },
  });
  fireEvent.change(screen.getByLabelText("Metric name"), {
    target: { value: "error_rate" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "600" },
  });
  fireEvent.change(screen.getByLabelText("Baseline offset (seconds)"), {
    target: { value: "3600" },
  });
  fireEvent.change(screen.getByLabelText("Threshold (%)"), {
    target: { value: "25" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Create Rule" }));

  await waitFor(() => expect(createMock).toHaveBeenCalledTimes(1));
  expect(createMock).toHaveBeenCalledWith(
    "test-tenant",
    expect.objectContaining({
      name: "Error rate shift",
      metric_name: "error_rate",
      alert_type: "change_detection",
      window_secs: 600,
      baseline_offset_secs: 3600,
      threshold_percent: 25,
    }),
  );
});

test("Change detection form rejects a blank metric name", async () => {
  mockRuntime([]);

  renderPage();

  await waitFor(() => screen.getByRole("button", { name: "New Rule" }));
  fireEvent.click(screen.getByRole("button", { name: "New Rule" }));
  fireEvent.change(screen.getByLabelText("Alert type"), {
    target: { value: "change_detection" },
  });
  fireEvent.change(screen.getByLabelText("Rule name"), {
    target: { value: "Error rate shift" },
  });
  fireEvent.change(screen.getByLabelText("Window (seconds)"), {
    target: { value: "600" },
  });
  fireEvent.change(screen.getByLabelText("Baseline offset (seconds)"), {
    target: { value: "3600" },
  });
  fireEvent.change(screen.getByLabelText("Threshold (%)"), {
    target: { value: "25" },
  });

  // The Metric name field carries an HTML `required` attribute, so a plain
  // click on the submit button is intercepted by jsdom's native constraint
  // validation before React's onSubmit handler ever runs. Dispatching the
  // submit event directly exercises the form's own validation logic
  // (handleCreateSubmit's "Metric name is required" check) instead.
  const form = screen.getByRole("form", { name: "Create alert rule" });
  fireEvent.submit(form);

  await waitFor(() =>
    expect(screen.getByText("Metric name is required")).toBeInTheDocument(),
  );
  expect(createMock).not.toHaveBeenCalled();
});

test("shows Suppressed badge for suppressed rules", async () => {
  mockRuntime([
    {
      rule_id: "rule-1",
      name: "CPU warning",
      metric_name: "cpu",
      operator: "gt" as const,
      threshold: 80,
      severity: "warning",
      silenced: false,
      state: "suppressed" as const,
      firing: false,
      last_fired_at: undefined,
      notification_channels: [],
      auto_trigger_incident: false,
      service_name: "payments",
      suppressed: true,
    },
  ]);

  renderPage();

  await waitFor(() => expect(screen.getByText("CPU warning")).toBeInTheDocument());
  expect(screen.getAllByText("Suppressed").length).toBeGreaterThan(0);
});
