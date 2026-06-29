import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface EmptyStateProps extends HTMLAttributes<HTMLDivElement> {
  title: string;
  description?: string;
  metadata?: string[];
  actions?: ReactNode;
  compact?: boolean;
}

export function EmptyState({
  title,
  description,
  metadata = [],
  actions,
  className,
  compact = false,
  ...props
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "modern-panel grid content-center justify-items-center gap-3 text-center",
        compact ? "min-h-0 p-4" : "min-h-[240px] p-7",
        className
      )}
      {...props}
    >
      <h2 className={cn("font-extrabold", compact ? "text-base" : "text-[22px]")}>{title}</h2>
      {description && <p className="m-0 max-w-xl text-sm text-[var(--muted)]">{description}</p>}
      {metadata.length > 0 && (
        <div className="flex flex-wrap justify-center gap-2 text-[13px] text-[var(--muted)]">
          {metadata.map((item) => (
            <span key={item} className="border border-[var(--border)] px-2 py-1.5">
              {item}
            </span>
          ))}
        </div>
      )}
      {actions && <div className="modern-toolbar justify-center">{actions}</div>}
    </div>
  );
}
