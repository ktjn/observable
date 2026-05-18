import { render, screen, fireEvent } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { PanelTemplateLibrary, PANEL_TEMPLATES } from "./PanelTemplateLibrary";

test("renders all templates and custom panel option", () => {
  render(
    <PanelTemplateLibrary
      onSelectTemplate={vi.fn()}
      onCustomPanel={vi.fn()}
    />,
  );

  for (const template of PANEL_TEMPLATES) {
    expect(screen.getByTestId(`template-${template.id}`)).toBeInTheDocument();
  }
  expect(screen.getByTestId("template-custom")).toBeInTheDocument();
});

test("calls onSelectTemplate when a template is clicked", () => {
  const onSelectTemplate = vi.fn();
  render(
    <PanelTemplateLibrary
      onSelectTemplate={onSelectTemplate}
      onCustomPanel={vi.fn()}
    />,
  );

  fireEvent.click(screen.getByTestId("template-error-rate"));
  expect(onSelectTemplate).toHaveBeenCalledTimes(1);
  expect(onSelectTemplate).toHaveBeenCalledWith(
    expect.objectContaining({
      id: "error-rate",
      title: "Error rate",
      query_kind: "metrics",
      query_text: "error rate over time",
    }),
  );
});

test("calls onCustomPanel when custom panel is clicked", () => {
  const onCustomPanel = vi.fn();
  render(
    <PanelTemplateLibrary
      onSelectTemplate={vi.fn()}
      onCustomPanel={onCustomPanel}
    />,
  );

  fireEvent.click(screen.getByTestId("template-custom"));
  expect(onCustomPanel).toHaveBeenCalledTimes(1);
});
