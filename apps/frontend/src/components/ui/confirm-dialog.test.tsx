import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ConfirmDialog } from "./confirm-dialog";

describe("ConfirmDialog", () => {
  it("renders alertdialog with consequences, focuses Cancel, and requires explicit confirmation", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog
        open
        title="Resolve incident"
        description='Resolve "Checkout latency spike"?'
        consequences="Stops active incident tracking."
        confirmLabel="Confirm resolve"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );

    const dialog = screen.getByRole("alertdialog", { name: "Resolve incident" });
    expect(dialog).toHaveTextContent("Stops active incident tracking.");
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Confirm resolve" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
