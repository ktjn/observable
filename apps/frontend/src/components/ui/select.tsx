import { forwardRef } from "react";
import type { OptionHTMLAttributes, SelectHTMLAttributes } from "react";
import { cn } from "./cn";

export type SelectProps = SelectHTMLAttributes<HTMLSelectElement>;

export type SelectOptionProps = OptionHTMLAttributes<HTMLOptionElement>;

const triggerClasses =
  "flex min-h-9 w-full rounded-md border border-[var(--border)] bg-[var(--surface)] px-3 text-sm text-[var(--text)] outline-none transition-colors focus-visible:ring-2 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-2 focus-visible:ring-offset-[var(--bg)] disabled:cursor-not-allowed disabled:opacity-60";

export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { className, children, ...props },
  ref
) {
  return (
    <select ref={ref} className={cn(triggerClasses, className)} {...props}>
      {children}
    </select>
  );
});

export function SelectOption(props: SelectOptionProps) {
  return <option {...props} />;
}
