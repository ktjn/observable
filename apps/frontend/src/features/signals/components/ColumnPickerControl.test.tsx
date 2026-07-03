import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ColumnPickerControl } from "./ColumnPickerControl";

test("toggling an unchecked column adds it to visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl visibleColumns={["service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Level"));

  expect(onChange).toHaveBeenCalledWith(["service", "level"]);
});

test("toggling a checked column removes it from visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl visibleColumns={["level", "service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Service"));

  expect(onChange).toHaveBeenCalledWith(["level"]);
});
