import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, test, vi } from "vitest";
import { QueryFilterInput } from "./QueryFilterInput";

vi.mock("../../api/nlq", () => ({
  submitNlqQuery: vi.fn(),
}));

import { submitNlqQuery } from "../../api/nlq";
const mockSubmit = vi.mocked(submitNlqQuery);

const SERVICES_BASE_IR = {
  operation: "catalog",
  signals: ["metrics"],
  filters: [],
  time_range: { from: "now-1h", to: "now" },
};

afterEach(() => {
  vi.clearAllMocks();
});

describe("QueryFilterInput", () => {
  test("submits natural language in interpret mode and calls onSubmit with raw text and onIr with IR", async () => {
    const onIr = vi.fn();
    const onSubmit = vi.fn();
    mockSubmit.mockResolvedValue({
      type: "ir",
      ir: {
        operation: "catalog",
        signals: ["metrics"],
        filters: [{ field: "service_name", op: "=", value: "checkout" }],
      },
    });

    render(<QueryFilterInput onIr={onIr} onSubmit={onSubmit} baseIr={SERVICES_BASE_IR} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: "show checkout services" },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    await waitFor(() =>
      expect(mockSubmit).toHaveBeenCalledWith({
        question: "show checkout services",
        mode: "interpret",
        service_name: undefined,
        base_ir: SERVICES_BASE_IR,
      }),
    );
    expect(onSubmit).toHaveBeenCalledWith("show checkout services");
    expect(onIr).toHaveBeenCalledWith({
      operation: "catalog",
      signals: ["metrics"],
      filters: [{ field: "service_name", op: "=", value: "checkout" }],
    });
  });

  test("raw IR JSON uses the same input and shows query details", async () => {
    const raw = JSON.stringify({
      operation: "catalog",
      signals: ["metrics"],
      filters: [{ field: "environment", op: "=", value: "prod" }],
    });
    mockSubmit.mockResolvedValue({
      type: "ir",
      ir: {
        operation: "catalog",
        signals: ["metrics"],
        filters: [{ field: "environment", op: "=", value: "prod" }],
      },
    });

    render(<QueryFilterInput onIr={vi.fn()} baseIr={SERVICES_BASE_IR} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Query current view input" }), {
      target: { value: raw },
    });
    fireEvent.submit(screen.getByRole("form", { name: "Query current view" }));

    expect(await screen.findByText("Show interpreted IR")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Show interpreted IR"));
    expect(screen.getByTestId("query-filter-ir")).toHaveTextContent("environment");
  });
});
