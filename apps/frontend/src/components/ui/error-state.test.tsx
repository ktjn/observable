import { render, screen } from "@testing-library/react";
import { ErrorState } from "./error-state";

test("renders default title, description, error, and actions", () => {
  render(
    <ErrorState
      title="Failed to load traces"
      description="The query timed out. Try narrowing the time range."
      error="TimeoutError: query exceeded 30s limit"
      actions={<button type="button">Retry</button>}
    />
  );

  expect(screen.getByRole("heading", { name: "Failed to load traces" })).toBeInTheDocument();
  expect(screen.getByText("The query timed out. Try narrowing the time range.")).toBeInTheDocument();
  expect(screen.getByText("TimeoutError: query exceeded 30s limit")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
});

test("renders default fallback title when none provided", () => {
  render(<ErrorState />);
  expect(screen.getByRole("heading", { name: "Something went wrong" })).toBeInTheDocument();
});

test("renders with role alert", () => {
  render(<ErrorState title="Error" />);
  expect(screen.getByRole("alert")).toBeInTheDocument();
});
