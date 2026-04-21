import { fireEvent, render, screen, within } from "@testing-library/react";
import App from "./App";
import { THEME_STORAGE_KEY } from "./lib/theme";

beforeEach(() => {
  window.localStorage.clear();
  window.history.pushState({}, "", "/");
});

test("renders the product navigation shell", async () => {
  render(<App />);

  const navigation = await screen.findByLabelText("Primary navigation");
  expect(within(navigation).getByRole("link", { name: "Services" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Infrastructure" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Service Overview" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Dashboards" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Alerts & SLOs" })).toBeInTheDocument();
  expect(within(navigation).getByRole("link", { name: "Admin / Fleet / Billing" })).toBeInTheDocument();

  await screen.findByRole("heading", { name: "Services" });
});

test("persists the selected theme preference", async () => {
  render(<App />);

  fireEvent.click(screen.getByRole("radio", { name: "Dark" }));

  expect(window.localStorage.getItem(THEME_STORAGE_KEY)).toBe("dark");
  expect(document.documentElement.dataset.themePreference).toBe("dark");
  expect(document.documentElement.dataset.theme).toBe("dark");
});
