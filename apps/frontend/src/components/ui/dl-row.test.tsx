import { render, screen } from "@testing-library/react";
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
