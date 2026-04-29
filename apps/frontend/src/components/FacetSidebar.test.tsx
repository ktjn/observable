import { render, screen, fireEvent } from "@testing-library/react";
import { vi } from "vitest";
import { FacetSidebar } from "./FacetSidebar";

const sampleFacets = {
  service_name: [
    { value: "checkout", count: 42 },
    { value: "payments", count: 18 },
  ],
  status_code: [
    { value: "OK", count: 55 },
    { value: "ERROR", count: 5 },
  ],
};

test("renders nothing when facets are undefined", () => {
  const { container } = render(
    <FacetSidebar facets={undefined} onFacetClick={vi.fn()} />
  );
  expect(container.firstChild).toBeNull();
});

test("renders nothing when facets object is empty", () => {
  const { container } = render(
    <FacetSidebar facets={{}} onFacetClick={vi.fn()} />
  );
  expect(container.firstChild).toBeNull();
});

test("renders facet groups with field headings and values", () => {
  render(<FacetSidebar facets={sampleFacets} onFacetClick={vi.fn()} />);

  expect(screen.getByText(/service name/i)).toBeInTheDocument();
  expect(screen.getByText("checkout")).toBeInTheDocument();
  expect(screen.getByText("42")).toBeInTheDocument();
  expect(screen.getByText("payments")).toBeInTheDocument();
  expect(screen.getByText("18")).toBeInTheDocument();
});

test("clicking a facet value calls onFacetClick with field and value", () => {
  const handler = vi.fn();
  render(<FacetSidebar facets={sampleFacets} onFacetClick={handler} />);

  fireEvent.click(screen.getByText("checkout"));
  expect(handler).toHaveBeenCalledOnce();
  expect(handler).toHaveBeenCalledWith("service_name", "checkout");
});

test("pressing Enter on a facet item triggers onFacetClick", () => {
  const handler = vi.fn();
  render(<FacetSidebar facets={sampleFacets} onFacetClick={handler} />);

  const checkoutItem = screen.getByText("checkout").closest('[role="button"]')!;
  fireEvent.keyDown(checkoutItem, { key: "Enter" });
  expect(handler).toHaveBeenCalledWith("service_name", "checkout");
});

test("pressing Space on a facet item triggers onFacetClick", () => {
  const handler = vi.fn();
  render(<FacetSidebar facets={sampleFacets} onFacetClick={handler} />);

  const okItem = screen.getByText("OK").closest('[role="button"]')!;
  fireEvent.keyDown(okItem, { key: " " });
  expect(handler).toHaveBeenCalledWith("status_code", "OK");
});

test("facet items are keyboard-focusable (tabIndex=0)", () => {
  render(<FacetSidebar facets={sampleFacets} onFacetClick={vi.fn()} />);
  const items = screen.getAllByRole("button");
  for (const item of items) {
    expect(item).toHaveAttribute("tabindex", "0");
  }
});
