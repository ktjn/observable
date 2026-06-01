import { render, screen, fireEvent } from "@testing-library/react";
import { expect, test, vi, describe, beforeEach } from "vitest";
import { TreeNav, type NavTreeItem } from "./TreeNav";

const testItems: NavTreeItem[] = [
  { id: "home", label: "Home", to: "/" },
  { id: "setup", label: "Setup", to: "/setup" },
  {
    id: "signals",
    label: "Signals",
    children: [
      { id: "traces", label: "Traces", to: "/traces" },
      { id: "logs", label: "Logs", to: "/logs" },
    ],
  },
  {
    id: "admin",
    label: "Administration",
    children: [
      { id: "overview", label: "Overview", to: "/admin" },
      { id: "config", label: "Tenant configuration", to: "/admin/config" },
      { id: "fleet", label: "Fleet management", to: "/admin/fleet" },
      { id: "identity", label: "Identity", to: "/admin/identity" },
    ],
  },
];

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    className,
    children,
  }: {
    to: string;
    className?: string;
    children: React.ReactNode;
  }) => (
    <a href={to} className={className}>
      {children}
    </a>
  ),
  useLocation: () => ({ pathname: "/" }),
}));

describe("TreeNav", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  test("renders all top-level items", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    expect(screen.getByText("Home")).toBeInTheDocument();
    expect(screen.getByText("Setup")).toBeInTheDocument();
    expect(screen.getByText("Signals")).toBeInTheDocument();
    expect(screen.getByText("Administration")).toBeInTheDocument();
  });

  test("does not render children by default", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    expect(screen.queryByText("Traces")).not.toBeInTheDocument();
    expect(screen.queryByText("Logs")).not.toBeInTheDocument();
    expect(screen.queryByText("Overview")).not.toBeInTheDocument();
    expect(screen.queryByText("Tenant configuration")).not.toBeInTheDocument();
    expect(screen.queryByText("Fleet management")).not.toBeInTheDocument();
    expect(screen.queryByText("Identity")).not.toBeInTheDocument();
  });

  test("expands parent when toggle is clicked", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    const signalsNode = screen.getByText("Signals").closest(".tree-node")!;
    const signalsToggle = signalsNode.querySelector("[aria-expanded='false']")!;
    fireEvent.click(signalsToggle);

    expect(screen.getByText("Traces")).toBeInTheDocument();
    expect(screen.getByText("Logs")).toBeInTheDocument();
  });

  test("collapses parent when toggle is clicked again", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    const signalsNode = screen.getByText("Signals").closest(".tree-node")!;
    const signalsToggle = signalsNode.querySelector("[aria-expanded='false']")!;
    fireEvent.click(signalsToggle);
    expect(screen.getByText("Traces")).toBeInTheDocument();

    const collapseToggle = signalsNode.querySelector("[aria-expanded='true']")!;
    fireEvent.click(collapseToggle);

    expect(screen.queryByText("Traces")).not.toBeInTheDocument();
    expect(screen.queryByText("Logs")).not.toBeInTheDocument();
  });

  test("auto-expands parent of active child route", () => {
    render(<TreeNav items={testItems} pathname="/logs" />);

    expect(screen.getByText("Logs")).toBeInTheDocument();
    expect(screen.getByText("Traces")).toBeInTheDocument();
  });

  test("marks active leaf route", () => {
    render(<TreeNav items={testItems} pathname="/setup" />);

    const setupLink = screen.getByText("Setup").closest("a");
    expect(setupLink).toHaveClass("active");
  });

  test("marks parent with has-active-child when child route is active", () => {
    render(<TreeNav items={testItems} pathname="/admin/config" />);

    const adminLabel = screen.getByText("Administration").closest("button");
    expect(adminLabel).toHaveClass("has-active-child");
    expect(adminLabel).not.toHaveClass("active");
  });

  test("renders links for items with 'to' prop", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    expect(screen.getByText("Home").closest("a")).toHaveAttribute("href", "/");
    expect(screen.getByText("Setup").closest("a")).toHaveAttribute(
      "href",
      "/setup",
    );
  });

  test("does not render link for parent-only items", () => {
    render(<TreeNav items={testItems} pathname="/" />);

    expect(screen.getByText("Signals").closest("a")).toBeNull();
    expect(screen.getByText("Signals").closest("button")).toBeInTheDocument();
  });
});
