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

test("clicking the promote button calls onPromote once", () => {
  const onPromote = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onPromote={onPromote}>
        TimeoutError
      </DlRow>
    </dl>
  );

  fireEvent.click(screen.getByRole("button", { name: "Add log.error.type as a column" }));
  expect(onPromote).toHaveBeenCalledTimes(1);
});

test("promoted rows disable the promote button and don't call onPromote again", () => {
  const onPromote = vi.fn();
  render(
    <dl>
      <DlRow label="log.error.type" onPromote={onPromote} promoted>
        TimeoutError
      </DlRow>
    </dl>
  );

  const button = screen.getByRole("button", { name: "log.error.type is already a column" });
  expect(button).toBeDisabled();
  fireEvent.click(button);
  expect(onPromote).not.toHaveBeenCalled();
});
