import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { ErrorState } from "./error-state";

describe("ErrorState", () => {
  it("renders alert role with retry button and support route", () => {
    const onRetry = vi.fn();
    render(
      <ErrorState
        title="Failed to load response time history"
        description="Latency series could not be retrieved."
        onRetry={onRetry}
        supportHref="/status"
      />,
    );

    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Failed to load response time history");
    expect(screen.getByRole("link", { name: "View system status" })).toHaveAttribute("href", "/status");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});
