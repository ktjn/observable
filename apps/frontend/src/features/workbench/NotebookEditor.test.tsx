import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, test, vi, afterEach } from "vitest";
import { NotebookEditor } from "./NotebookEditor";

const editorProps: Array<Record<string, unknown>> = [];

vi.mock("@monaco-editor/react", () => ({
  default: (props: Record<string, unknown>) => {
    editorProps.push(props);
    return (
      <textarea
        data-testid="monaco-editor"
        value={String(props.value ?? "")}
        onChange={(event) => (props.onChange as ((value: string) => void) | undefined)?.(event.target.value)}
      />
    );
  },
}));

afterEach(() => {
  editorProps.length = 0;
});

describe("NotebookEditor", () => {
  test("uses plaintext for NLQ mode and json for raw mode", () => {
    render(<NotebookEditor value="latency" mode="nlq" onChange={vi.fn()} />);
    render(<NotebookEditor value='{"operation":"table"}' mode="raw" onChange={vi.fn()} />);

    expect(screen.getAllByTestId("monaco-editor")).toHaveLength(2);
    expect(editorProps[0]?.language).toBe("plaintext");
    expect(editorProps[1]?.language).toBe("json");
  });

  test("propagates editor changes back to the parent", () => {
    const onChange = vi.fn();
    render(<NotebookEditor value="" mode="nlq" onChange={onChange} />);

    fireEvent.change(screen.getByTestId("monaco-editor"), {
      target: { value: "p95 latency" },
    });

    expect(onChange).toHaveBeenCalledWith("p95 latency");
  });
});
