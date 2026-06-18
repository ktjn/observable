import { forwardRef } from "react";
import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

type ButtonVariant = "primary" | "secondary" | "destructive" | "ghost";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
}

const variantClasses: Record<ButtonVariant, string> = {
  primary:
    "bg-[var(--accent)] text-white hover:bg-[var(--accent-strong)] disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
  secondary:
    "border border-[var(--border-strong)] bg-[var(--surface)] text-[var(--text)] hover:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
  destructive:
    "bg-[var(--bad)] text-white hover:opacity-90 disabled:bg-[var(--surface-subtle)] disabled:text-[var(--muted)]",
  ghost:
    "bg-transparent text-[var(--muted)] hover:bg-[var(--surface-subtle)] hover:text-[var(--text)] disabled:text-[var(--muted)]",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, type = "button", variant = "primary", ...props },
  ref
) {
  return (
    <button
      ref={ref}
      type={type}
      className={cn(
        "inline-flex min-h-7 items-center justify-center border-0 px-3 text-[11px] font-semibold outline-none transition-colors",
        "focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] focus-visible:ring-offset-1 focus-visible:ring-offset-[var(--bg)]",
        "disabled:cursor-not-allowed",
        variantClasses[variant],
        className
      )}
      {...props}
    />
  );
});
