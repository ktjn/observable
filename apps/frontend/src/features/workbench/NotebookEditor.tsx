import Editor from "@monaco-editor/react";
import { useId } from "react";
import type { WorkbenchMode } from "./workbenchState";
import { ShorthandHint } from "../nlq/ShorthandHint";

interface Props {
  value: string;
  mode: WorkbenchMode;
  onChange: (value: string) => void;
  disabled?: boolean;
}

export function NotebookEditor({ value, mode, onChange, disabled = false }: Props) {
  const labelId = useId();
  const language = mode === "raw" ? "json" : "plaintext";

  const editor = (
    <div className="overflow-hidden rounded border border-[var(--border)] bg-[var(--surface)]">
      <Editor
        value={value}
        language={language}
        onChange={(next) => onChange(next ?? "")}
        height="180px"
        options={{
          readOnly: disabled,
          minimap: { enabled: false },
          wordWrap: "on",
          scrollBeyondLastLine: false,
          fontSize: 13,
          lineNumbers: "off",
          tabSize: 2,
          automaticLayout: true,
        }}
        loading={<div className="p-3 text-sm text-[var(--text-muted)]">Loading editor…</div>}
        className="workbench-monaco"
        onMount={(editor) => {
          if (!disabled) {
            editor.updateOptions({ readOnly: false });
          }
        }}
      />
    </div>
  );

  return (
    <div className="space-y-2" data-testid="workbench-editor" role="group" aria-labelledby={labelId}>
      <div className="text-xs font-medium text-[var(--text-muted)]" id={labelId}>
        {mode === "raw" ? "Raw IR" : "Natural-language query"}
      </div>
      {mode === "nlq" ? (
        <ShorthandHint className="relative group">{editor}</ShorthandHint>
      ) : (
        editor
      )}
    </div>
  );
}
