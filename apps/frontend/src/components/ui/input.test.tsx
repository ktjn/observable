import { render, screen } from "@testing-library/react";
import { Input } from "./input";

test("renders a text input with placeholder", () => {
  render(<Input placeholder="Search services" />);
  expect(screen.getByPlaceholderText("Search services")).toHaveAttribute(
    "type",
    "text"
  );
});

test("respects disabled state", () => {
  render(<Input aria-label="Search" disabled />);
  expect(screen.getByRole("textbox", { name: "Search" })).toBeDisabled();
});
