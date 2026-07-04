import { render, screen, fireEvent } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { CopyButton, CopyableText } from "./copy-button";

let writeText: ReturnType<typeof vi.fn>;

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  writeText = vi.fn().mockResolvedValue(undefined);
  Object.assign(navigator, { clipboard: { writeText } });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  delete document.documentElement.dataset.theme;
});

test("click copies the value and shows a Copied state that reverts", async () => {
  render(<CopyButton value="trace-123" label="Copy trace id" />);
  const button = screen.getByRole("button", { name: "Copy trace id" });

  fireEvent.click(button);
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith("trace-123"));
  expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();

  vi.advanceTimersByTime(1500);
  expect(
    await screen.findByRole("button", { name: "Copy trace id" })
  ).toBeInTheDocument();
});

test("blurs itself after a click so it doesn't stay visible via :focus-within forever", async () => {
  render(
    <div className="group">
      <CopyButton value="trace-123" label="Copy trace id" />
    </div>
  );
  const button = screen.getByRole("button", { name: "Copy trace id" });
  button.focus();
  expect(button).toHaveFocus();

  fireEvent.click(button);

  expect(button).not.toHaveFocus();
});

test("is a real button, activatable via keyboard", () => {
  render(<CopyButton value="abc" />);
  const button = screen.getByRole("button", { name: "Copy" });
  expect(button.tagName).toBe("BUTTON");
  expect(button).toHaveAttribute("type", "button");
});

test("falls back to execCommand when navigator.clipboard is unavailable", async () => {
  Object.assign(navigator, { clipboard: undefined });
  const execCommand = vi.fn().mockReturnValue(true);
  document.execCommand = execCommand;

  render(<CopyButton value="fallback-value" />);
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));

  await vi.waitFor(() => expect(execCommand).toHaveBeenCalledWith("copy"));
  expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
});

test("logs and recovers when both copy paths fail", async () => {
  const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
  writeText.mockRejectedValue(new Error("denied"));

  render(<CopyButton value="oops" />);
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));

  await vi.waitFor(() => expect(consoleError).toHaveBeenCalled());
  vi.advanceTimersByTime(1500);
  expect(
    await screen.findByRole("button", { name: "Copy" })
  ).toBeInTheDocument();
});

test("clicking does not bubble to an ancestor click handler", () => {
  const onAncestorClick = vi.fn();
  render(
    <div onClick={onAncestorClick}>
      <CopyButton value="row-value" />
    </div>
  );
  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  expect(onAncestorClick).not.toHaveBeenCalled();
});

test("CopyableText renders display children and copies the underlying value", async () => {
  render(
    <CopyableText value="full-id-value" mono>
      full-id…
    </CopyableText>
  );
  expect(screen.getByText("full-id…")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Copy" }));
  await vi.waitFor(() => expect(writeText).toHaveBeenCalledWith("full-id-value"));
});
