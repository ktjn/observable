import type { FormEvent } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { ShorthandHint } from "../../features/nlq/ShorthandHint";

interface SignalQueryFormProps {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (event: FormEvent) => void;
  isLoading: boolean;
  inputLabel: string;
  formLabel: string;
  placeholder: string;
  idleLabel: string;
  loadingLabel: string;
  inputTestId?: string;
  submitTestId?: string;
}

export function SignalQueryForm({
  value,
  onChange,
  onSubmit,
  isLoading,
  inputLabel,
  formLabel,
  placeholder,
  idleLabel,
  loadingLabel,
  inputTestId,
  submitTestId,
}: SignalQueryFormProps) {
  return (
    <form
      aria-label={formLabel}
      role="form"
      onSubmit={onSubmit}
      className="flex gap-2 max-[640px]:flex-col"
    >
      <ShorthandHint className="relative z-30 group min-w-[260px] flex-1">
        <Input
          aria-label={inputLabel}
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder={placeholder}
          disabled={isLoading}
          className="w-full"
          data-testid={inputTestId}
        />
      </ShorthandHint>
      <Button
        type="submit"
        disabled={isLoading || !value.trim()}
        data-testid={submitTestId}
      >
        {isLoading ? loadingLabel : idleLabel}
      </Button>
    </form>
  );
}
