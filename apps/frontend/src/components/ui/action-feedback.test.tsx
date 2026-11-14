import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ActionFeedback } from "./action-feedback";

describe("ActionFeedback", () => {
  it("renders nothing when idle", () => {
    const { container } = render(<ActionFeedback status="idle" message="" />);
    expect(container).toBeEmptyDOMElement();
  });

  it("announces pending and completed states via status live region", () => {
    const { rerender } = render(
      <ActionFeedback status="pending" message="Acknowledging incident…" />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("In progress");
    expect(screen.getByRole("status")).toHaveTextContent("Acknowledging incident…");

    rerender(<ActionFeedback status="success" message="Incident acknowledged" />);
    expect(screen.getByRole("status")).toHaveTextContent("Completed");
    expect(screen.getByRole("status")).toHaveTextContent("Incident acknowledged");
  });

  it("announces failed state via alert with retry and never shows Completed", () => {
    const onRetry = vi.fn();
    render(
      <ActionFeedback
        status="error"
        message="Acknowledgement failed"
        detail="500 Internal Server Error"
        onRetry={onRetry}
      />,
    );
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Failed");
    expect(alert).toHaveTextContent("Acknowledgement failed");
    expect(alert).not.toHaveTextContent("Completed");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});
