import { render, screen } from "@testing-library/react";
import { Panel } from "./panel";

test("renders title and children", () => {
  render(
    <Panel title="Service health">
      <div>Latency summary</div>
    </Panel>
  );

  expect(screen.getByRole("heading", { name: "Service health" })).toBeInTheDocument();
  expect(screen.getByText("Latency summary")).toBeInTheDocument();
});

test("renders optional actions", () => {
  render(
    <Panel title="Services" actions={<button type="button">Refresh</button>}>
      Body
    </Panel>
  );

  expect(screen.getByRole("button", { name: "Refresh" })).toBeInTheDocument();
});
