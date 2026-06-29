import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, test, vi, beforeEach } from "vitest";
import { CommandPalette } from "./command-palette";

const mockNavigate = vi.fn();

vi.mock("@tanstack/react-router", () => ({
  useRouter: () => ({ navigate: mockNavigate }),
}));

beforeEach(() => {
  mockNavigate.mockClear();
});

test("renders nothing when open is false", () => {
  render(<CommandPalette open={false} onClose={vi.fn()} />);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("renders dialog when open is true", () => {
  render(<CommandPalette open={true} onClose={vi.fn()} />);
  expect(screen.getByRole("dialog")).toBeInTheDocument();
});

test("has role='dialog' and aria-label='Command palette' when open", () => {
  render(<CommandPalette open={true} onClose={vi.fn()} />);
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveAttribute("aria-label", "Command palette");
  expect(dialog).toHaveAttribute("aria-modal", "true");
});

test("pressing Escape calls onClose", () => {
  const onClose = vi.fn();
  render(<CommandPalette open={true} onClose={onClose} />);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(onClose).toHaveBeenCalledOnce();
});

describe("result filtering", () => {
  test("typing 'trace' shows Traces page navigation result", () => {
    render(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "trace" } });
    expect(screen.getByText("Traces")).toBeInTheDocument();
  });

  test("typing a 32-char hex string shows trace lookup result", () => {
    render(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("combobox");
    const traceId = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4";
    fireEvent.change(input, { target: { value: traceId } });
    expect(screen.getByText(`Go to trace: ${traceId}`)).toBeInTheDocument();
  });

  test("typing a non-hex string does NOT show trace lookup", () => {
    render(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "my-service-name" } });
    expect(screen.queryByText(/Go to trace:/)).not.toBeInTheDocument();
  });

  test("typing a hex string shorter than 16 chars does NOT show trace lookup", () => {
    render(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "a1b2c3d4" } });
    expect(screen.queryByText(/Go to trace:/)).not.toBeInTheDocument();
  });
});

test("shows 'No results' when input matches nothing", () => {
  render(<CommandPalette open={true} onClose={vi.fn()} />);
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "zzzzzzzzzzzzz" } });
  expect(screen.getByText("No results")).toBeInTheDocument();
});

test("clicking a page result calls router navigate and onClose", () => {
  const onClose = vi.fn();
  render(<CommandPalette open={true} onClose={onClose} />);
  const input = screen.getByRole("combobox");
  fireEvent.change(input, { target: { value: "services" } });
  const servicesItem = screen.getByText("Services");
  fireEvent.click(servicesItem);
  expect(mockNavigate).toHaveBeenCalledWith({ to: "/services" });
  expect(onClose).toHaveBeenCalledOnce();
});

test("clicking a trace lookup result navigates to /traces/:traceId", () => {
  const onClose = vi.fn();
  render(<CommandPalette open={true} onClose={onClose} />);
  const input = screen.getByRole("combobox");
  const traceId = "deadbeefdeadbeefdeadbeefdeadbeef";
  fireEvent.change(input, { target: { value: traceId } });
  const traceItem = screen.getByText(`Go to trace: ${traceId}`);
  fireEvent.click(traceItem);
  expect(mockNavigate).toHaveBeenCalledWith({ to: `/traces/${traceId}` });
  expect(onClose).toHaveBeenCalledOnce();
});
