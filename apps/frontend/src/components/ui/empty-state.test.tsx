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
