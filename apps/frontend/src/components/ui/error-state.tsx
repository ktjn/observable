import type { HTMLAttributes, ReactNode } from "react";
import { Button } from "./button";
import { cn } from "./cn";

export interface ErrorStateProps extends HTMLAttributes<HTMLDivElement> {
  title?: string;
  description?: string;
  error?: string;
  actions?: ReactNode;
  onRetry?: () => void;
  retryLabel?: string;
  supportHref?: string;
  supportLabel?: string;
}

export function ErrorState({
  title = "Something went wrong",
  description,
  error,
  actions,
  onRetry,
  retryLabel = "Retry",
  supportHref,
  supportLabel = "View system status",
  className,
  ...props
}: ErrorStateProps) {
  const hasActions = Boolean(actions || onRetry || supportHref);
  return (
    <div
      className={cn(
        "modern-panel grid min-h-[160px] content-center justify-items-center gap-3 p-7 text-center border border-[var(--bad)]",
        className
      )}
      role="alert"
      {...props}
    >
      <div className="flex items-center gap-2">
        <svg
          width="20"
          height="20"
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden="true"
          className="text-[var(--bad)] shrink-0"
        >
          <circle cx="10" cy="10" r="9" />
          <line x1="10" y1="6" x2="10" y2="11" />
          <circle cx="10" cy="14" r="1" fill="currentColor" stroke="none" />
        </svg>
        <h2 className="text-[22px] font-extrabold text-[var(--bad)]">{title}</h2>
      </div>
      {description && <p className="m-0 max-w-xl text-sm text-[var(--muted)]">{description}</p>}
      {error && (
        <pre className="m-0 max-w-xl text-xs text-[var(--bad)] whitespace-pre-wrap font-mono bg-[var(--bad-bg)] px-3 py-2 rounded">
          {error}
        </pre>
      )}
      {hasActions && (
        <div className="modern-toolbar flex flex-wrap items-center justify-center gap-2">
          {onRetry && (
            <Button type="button" variant="secondary" onClick={onRetry}>
              {retryLabel}
            </Button>
          )}
          {supportHref && (
            <a href={supportHref} className="text-xs font-medium text-[var(--brand)] underline">
              {supportLabel}
            </a>
          )}
          {actions}
        </div>
      )}
    </div>
  );
}
