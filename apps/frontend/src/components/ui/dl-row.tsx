import type { ReactNode } from "react";
import { Minus, Plus } from "lucide-react";
import { CopyButton } from "./copy-button";

export interface DlRowProps {
  label: string;
  children: ReactNode;
  copyValue?: string;
  /** Adds an affordance to toggle this property as a table column. Omit when not applicable. */
  onToggleColumn?: () => void;
  /** Whether this property is currently visible as a table column. */
  columnVisible?: boolean;
}

export function DlRow({ label, children, copyValue, onToggleColumn, columnVisible }: DlRowProps) {
  const toggleLabel = columnVisible ? `Remove ${label} column` : `Add ${label} as a column`;

  return (
    <div className="contents group">
      <dt className="flex items-start gap-1 break-all font-bold text-[var(--muted)]">
        {label}
        {onToggleColumn && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onToggleColumn();
              e.currentTarget.blur();
            }}
            title={toggleLabel}
            aria-label={toggleLabel}
            className="inline-flex shrink-0 items-center justify-center text-[var(--muted)] outline-none transition-opacity opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] enabled:hover:text-[var(--brand)] disabled:opacity-40"
          >
            {columnVisible ? <Minus className="size-3" /> : <Plus className="size-3" />}
          </button>
        )}
      </dt>
      <dd className="m-0 flex min-w-0 items-start gap-1 break-all text-[var(--text)]">
        {children}
        {copyValue !== undefined && <CopyButton value={copyValue} size="xs" />}
      </dd>
    </div>
  );
}
