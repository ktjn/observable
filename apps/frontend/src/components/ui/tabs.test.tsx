import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Tabs } from "./tabs";

function TestTabs() {
  return (
    <Tabs.Root defaultValue="logs">
      <Tabs.List aria-label="Service signals">
        <Tabs.Tab value="logs">Logs</Tabs.Tab>
        <Tabs.Tab value="metrics">Metrics</Tabs.Tab>
      </Tabs.List>
      <Tabs.Panel value="logs">Logs panel</Tabs.Panel>
      <Tabs.Panel value="metrics">Metrics panel</Tabs.Panel>
    </Tabs.Root>
  );
}

test("renders the default tab panel", () => {
  render(<TestTabs />);
  expect(screen.getByText("Logs panel")).toBeVisible();
});

test("switches tabs by click", () => {
  render(<TestTabs />);
  fireEvent.click(screen.getByRole("tab", { name: "Metrics" }));
  expect(screen.getByText("Metrics panel")).toBeVisible();
});

test("supports keyboard navigation", async () => {
  render(<TestTabs />);
  const logs = screen.getByRole("tab", { name: "Logs" });
  logs.focus();
  fireEvent.keyDown(logs, { key: "ArrowRight" });
  await waitFor(() =>
    expect(screen.getByRole("tab", { name: "Metrics" })).toHaveFocus()
  );
});
