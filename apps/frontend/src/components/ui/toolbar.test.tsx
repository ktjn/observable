import { render, screen } from "@testing-library/react";
import { Toolbar } from "./toolbar";

test("renders as an accessible toolbar", () => {
  render(
    <Toolbar aria-label="Service filters">
      <button type="button">Refresh</button>
    </Toolbar>
  );

  expect(screen.getByRole("toolbar", { name: "Service filters" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
});
