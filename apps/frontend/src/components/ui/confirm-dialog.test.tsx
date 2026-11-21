import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ConfirmDialog } from "./confirm-dialog";

describe("ConfirmDialog", () => {
  it("renders alertdialog with consequences, focuses Cancel, and requires explicit confirmation", async () => {
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
    // Base UI's floating-focus-manager applies initial focus post-paint (RAF), not
    // synchronously during the mount effect like the old hand-rolled implementation.
    await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus());
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(onConfirm).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Confirm resolve" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });
});
