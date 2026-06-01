import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import HomePage from "./HomePage";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Link: ({
      children,
      to,
      ...props
    }: React.AnchorHTMLAttributes<HTMLAnchorElement> & { to?: string; children?: React.ReactNode }) => (
      <a href={to ?? "#"} {...props}>
        {children}
      </a>
    ),
  };
});

vi.mock("@tanstack/react-query", () => ({
  useQuery: () => ({ data: { items: [] }, isLoading: false }),
}));

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({ tenantId: "test-tenant" }),
}));

describe("HomePage navigation", () => {
  test("points the quick nav at the workbench", () => {
    render(<HomePage />);

    expect(screen.getByRole("link", { name: "Workbench" })).toHaveAttribute("href", "/workbench");
    expect(screen.queryByRole("link", { name: "Ask AI (NLQ)" })).not.toBeInTheDocument();
  });
});
