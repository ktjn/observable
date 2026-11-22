import { render, screen } from "@testing-library/react";
import { afterEach } from "vitest";
import { Button } from "./button";

afterEach(() => {
  delete document.documentElement.dataset.theme;
});

test("renders button content and default type", () => {
  render(<Button>Save</Button>);
  const button = screen.getByRole("button", { name: "Save" });
  expect(button).toHaveAttribute("type", "button");
});

test("applies disabled state", () => {
  render(<Button disabled>Delete</Button>);
  expect(screen.getByRole("button", { name: "Delete" })).toBeDisabled();
});

test("renders under the dark theme contract", () => {
  document.documentElement.dataset.theme = "dark";
  render(<Button>Dark action</Button>);
  expect(screen.getByRole("button", { name: "Dark action" })).toBeInTheDocument();
});

test("renders under the light theme contract", () => {
  document.documentElement.dataset.theme = "light";
  render(<Button>Light action</Button>);
  expect(screen.getByRole("button", { name: "Light action" })).toBeInTheDocument();
});

test("destructive variant maps onto Designed's danger variant class", () => {
  render(<Button variant="destructive">Delete</Button>);
  const destructiveButton = screen.getByRole("button", { name: "Delete" });

  render(<Button variant="primary">Save</Button>);
  const primaryButton = screen.getByRole("button", { name: "Save" });

  expect(destructiveButton.className).not.toBe(primaryButton.className);
});

test("preserves a consumer class override alongside the variant classes", () => {
  render(<Button className="extra-class">Save</Button>);
  expect(screen.getByRole("button", { name: "Save" }).className).toContain("extra-class");
});
