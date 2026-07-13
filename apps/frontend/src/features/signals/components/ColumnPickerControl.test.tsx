import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { ColumnPickerControl } from "./ColumnPickerControl";

const LOG_COLUMNS = [
  { key: "level", label: "Level" },
  { key: "service", label: "Service" },
];

function rowFor(labelText: string): HTMLElement {
  return screen.getByLabelText(labelText).closest("[draggable]") as HTMLElement;
}

test("toggling an unchecked column calls onToggle with its key", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl columns={LOG_COLUMNS} visibleColumns={["service"]} onToggle={onToggle} onReorder={vi.fn()} />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Level"));

  expect(onToggle).toHaveBeenCalledWith("level");
});

test("toggling a checked column calls onToggle with its key", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={onToggle}
      onReorder={vi.fn()}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.click(screen.getByLabelText("Service"));

  expect(onToggle).toHaveBeenCalledWith("service");
});

test("renders arbitrary promoted columns alongside fixed ones", () => {
  const onToggle = vi.fn();
  render(
    <ColumnPickerControl
      columns={[...LOG_COLUMNS, { key: "log.error.type", label: "log.error.type" }]}
      visibleColumns={["level", "service", "log.error.type"]}
      onToggle={onToggle}
      onReorder={vi.fn()}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  expect(screen.getByLabelText("log.error.type")).toBeChecked();

  fireEvent.click(screen.getByLabelText("log.error.type"));
  expect(onToggle).toHaveBeenCalledWith("log.error.type");
});

test("dragging a row onto another calls onReorder with the new key order", () => {
  const onReorder = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={vi.fn()}
      onReorder={onReorder}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.dragStart(rowFor("Level"));
  fireEvent.dragOver(rowFor("Service"));
  fireEvent.drop(rowFor("Service"));

  expect(onReorder).toHaveBeenCalledWith(["service", "level"]);
});

test("dropping a row onto itself does not call onReorder", () => {
  const onReorder = vi.fn();
  render(
    <ColumnPickerControl
      columns={LOG_COLUMNS}
      visibleColumns={["level", "service"]}
      onToggle={vi.fn()}
      onReorder={onReorder}
    />
  );

  fireEvent.click(screen.getByRole("button", { name: /columns/i }));
  fireEvent.dragStart(rowFor("Level"));
  fireEvent.dragOver(rowFor("Level"));
  fireEvent.drop(rowFor("Level"));

  expect(onReorder).not.toHaveBeenCalled();
});
