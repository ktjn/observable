import { render, screen } from "@testing-library/react";
import { EmptyState } from "./empty-state";

test("renders title, description, metadata, and actions", () => {
  render(
    <EmptyState
      title="No services found"
      description="Adjust filters or send telemetry."
      metadata={["Tenant: local-dev", "Range: Last 1h"]}
      actions={<button type="button">Open setup</button>}
    />
  );

  expect(screen.getByRole("heading", { name: "No services found" })).toBeInTheDocument();
  expect(screen.getByText("Adjust filters or send telemetry.")).toBeInTheDocument();
  expect(screen.getByText("Tenant: local-dev")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open setup" })).toBeInTheDocument();
});

test("default (non-compact) still has min-h-[240px]", () => {
  const { container } = render(<EmptyState title="Test" />);
  expect(container.firstChild).toHaveClass("min-h-[240px]");
});

test("compact prop removes min-height class", () => {
  const { container } = render(<EmptyState title="Test" compact />);
  expect(container.firstChild).not.toHaveClass("min-h-[240px]");
});

test("compact prop uses smaller padding class", () => {
  const { container } = render(<EmptyState title="Test" compact />);
  expect(container.firstChild).toHaveClass("p-4");
  expect(container.firstChild).not.toHaveClass("p-7");
});

test("compact prop uses smaller title font class", () => {
  const { container } = render(<EmptyState title="Test" compact />);
  const heading = container.querySelector("h2");
  expect(heading).toHaveClass("text-base");
  expect(heading).not.toHaveClass("text-[22px]");
});
