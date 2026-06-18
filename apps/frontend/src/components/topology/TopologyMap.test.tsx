import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { TopologyMap } from "./TopologyMap";
import type { TopologyEdge } from "../../api/services";

const noop = vi.fn();

describe("TopologyMap", () => {
  test("shows an empty-state message when there are 0 services", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={[]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/no service dependencies detected yet/i)).toBeInTheDocument();
  });

  test("shows an empty-state message when there is exactly 1 service and no edges", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={["checkout"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/no service dependencies detected yet/i)).toBeInTheDocument();
  });

  test("does not show the empty-state when there are 2+ connected services", () => {
    const edges: TopologyEdge[] = [
      { caller: "checkout", callee: "payments", request_count: 10, error_rate: 0.01, p95_latency_ms: 50 },
    ];
    render(
      <TopologyMap
        edges={edges}
        allServices={["checkout", "payments"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.queryByText(/no service dependencies detected yet/i)).not.toBeInTheDocument();
  });

  test("renders a legend", () => {
    render(
      <TopologyMap
        edges={[]}
        allServices={["checkout", "payments"]}
        focusedService={null}
        onNodeClick={noop}
        onEdgeClick={noop}
        onBackgroundClick={noop}
      />,
    );
    expect(screen.getByText(/error rate/i)).toBeInTheDocument();
  });
});
