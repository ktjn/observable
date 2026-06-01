import { render, screen } from "@testing-library/react";
import { describe, expect, test, vi } from "vitest";
import { AppShell } from "./AppShell";

vi.mock("@tanstack/react-router", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-router")>();
  return {
    ...actual,
    Outlet: () => <div data-testid="outlet" />,
    useLocation: () => ({ pathname: "/" }),
  };
});

vi.mock("@tanstack/react-query", () => ({
  useQuery: ({ queryKey }: { queryKey: unknown[] }) => {
    if (queryKey[0] === "tenants") {
      return { data: { tenants: [{ id: "test-tenant", name: "observable" }] } };
    }
    if (queryKey[0] === "environments") {
      return { data: { environments: [] } };
    }
    return { data: undefined };
  },
}));

vi.mock("../hooks/useTenantContext", () => ({
  useTenantContext: () => ({
    tenantId: "test-tenant",
    tenantName: "observable",
    environment: null,
    setTenant: vi.fn(),
    setEnvironment: vi.fn(),
  }),
}));

vi.mock("../hooks/useAuth", () => ({
  useAuth: () => ({ data: { user_id: "user-1" }, isLoading: false }),
}));

vi.mock("../api/auth", () => ({
  initiateLogin: vi.fn(),
}));

vi.mock("../lib/theme", () => ({
  useTheme: () => ({ preference: "system", setPreference: vi.fn() }),
}));

vi.mock("../lib/timeDisplay", () => ({
  useTimeDisplay: () => ({ format: "relative", setFormat: vi.fn() }),
  TIME_FORMAT_OPTIONS: [{ value: "relative", label: "Relative" }],
}));

vi.mock("../hooks/useGlobalDateRange", () => ({
  DEFAULT_PRESET: "1h",
  PRESET_OPTIONS: [{ value: "1h", label: "Last 1 hour" }],
  useGlobalDateRange: () => ({
    preset: "1h",
    fromMs: 1_700_000_000_000,
    toMs: 1_700_003_600_000,
    setPreset: vi.fn(),
    setCustomRange: vi.fn(),
    clearCustomRange: vi.fn(),
  }),
}));

vi.mock("../components/TreeNav", () => ({
  TreeNav: ({ items }: { items: Array<{ label: string; to?: string; children?: Array<{ label: string; to?: string }> }> }) => (
    <nav aria-label="mock nav">
      {items.flatMap((item) =>
        item.children
          ? item.children.map((child) => (
              <a key={child.label} href={child.to}>
                {child.label}
              </a>
            ))
          : [
              <a key={item.label} href={item.to}>
                {item.label}
              </a>,
            ],
      )}
    </nav>
  ),
}));

vi.mock("../components/UserMenu", () => ({
  UserMenu: () => <div data-testid="user-menu" />,
}));

describe("AppShell navigation", () => {
  test("points the primary notebook entry at /workbench", () => {
    render(<AppShell />);

    expect(screen.getByRole("link", { name: "Workbench" })).toHaveAttribute("href", "/workbench");
    expect(screen.queryByRole("link", { name: "Ask (NLQ)" })).not.toBeInTheDocument();
  });
});
