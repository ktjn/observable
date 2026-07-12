import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import { DlRow } from "./dl-row";

test("renders label and value without a copy button by default", () => {
  render(
    <dl>
      <DlRow label="service">checkout-api</DlRow>
    </dl>
  );
  expect(screen.getByText("service")).toBeInTheDocument();
  expect(screen.getByText("checkout-api")).toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
});

test("renders a copy button when copyValue is provided", () => {
  render(
    <dl>
      <DlRow label="trace_id" copyValue="trace-abc-123">
        trace-abc…
      </DlRow>
    </dl>
  );
  expect(
    screen.getByRole("button", { name: "Copy" })
  ).toBeInTheDocument();
});

test("clicking the add-column button calls onToggleColumn once", () => {
  const onToggleColumn = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onToggleColumn={onToggleColumn} columnVisible={false}>
        TimeoutError
      </DlRow>
    </dl>
  );

  const button = screen.getByRole("button", { name: "Add log.error.type as a column" });
  expect(button.querySelector(".lucide-plus")).toBeInTheDocument();
  fireEvent.click(button);
  expect(onToggleColumn).toHaveBeenCalledTimes(1);
});

test("clicking the remove-column button calls onToggleColumn once", () => {
  const onToggleColumn = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onToggleColumn={onToggleColumn} columnVisible={true}>
        TimeoutError
      </DlRow>
    </dl>
  );

  const button = screen.getByRole("button", { name: "Remove log.error.type column" });
  expect(button.querySelector(".lucide-minus")).toBeInTheDocument();
  fireEvent.click(button);
  expect(onToggleColumn).toHaveBeenCalledTimes(1);
});
