import type { ReactNode } from "react";
import { cn } from "./cn";

type LoadingVariant = "text" | "skeleton" | "spinner";

export function LoadingState({
  children,
  className,
  variant = "text",
  ...props
}: {
  children?: ReactNode;
  className?: string;
  variant?: LoadingVariant;
}) {
  if (variant === "skeleton") {
    return (
      <div
        aria-hidden="true"
        className={cn(
          "border border-[var(--border)] bg-[var(--surface)] animate-pulse",
          className
        )}
        {...props}
      />
    );
  }

  return (
    <div className={cn("p-12 text-center text-[var(--muted)]", className)} {...props}>
      {variant === "spinner" && (
        <span
          className="inline-block w-4 h-4 border-2 border-[var(--border)] border-t-[var(--brand)] rounded-full animate-spin mr-2 align-middle"
          aria-hidden="true"
        />
      )}
      {children}
    </div>
  );
}
