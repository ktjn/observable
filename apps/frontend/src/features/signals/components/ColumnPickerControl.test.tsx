import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ColumnPickerControl } from "./ColumnPickerControl";

const LOG_COLUMNS = [
  { key: "level", label: "Level" },
  { key: "service", label: "Service" },
];

test("toggling an unchecked column adds it to visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl columns={LOG_COLUMNS} visibleColumns={["service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Level"));

  expect(onChange).toHaveBeenCalledWith(["service", "level"]);
});

test("toggling a checked column removes it from visibleColumns", () => {
  const onChange = vi.fn();
  render(<ColumnPickerControl columns={LOG_COLUMNS} visibleColumns={["level", "service"]} onChange={onChange} />);

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Service"));

  expect(onChange).toHaveBeenCalledWith(["level"]);
});

test("renders arbitrary promoted columns alongside fixed ones", () => {
  const onChange = vi.fn();
  render(
    <ColumnPickerControl
      columns={[...LOG_COLUMNS, { key: "log.error.type", label: "log.error.type" }]}
      visibleColumns={["level", "service", "log.error.type"]}
      onChange={onChange}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  expect(screen.getByLabelText("log.error.type")).toBeChecked();

  fireEvent.click(screen.getByLabelText("log.error.type"));
  expect(onChange).toHaveBeenCalledWith(["level", "service"]);
});
