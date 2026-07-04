import type { ReactNode } from "react";
import { Plus } from "lucide-react";
import { CopyButton } from "./copy-button";

export interface DlRowProps {
  label: string;
  children: ReactNode;
  copyValue?: string;
  /** Adds a "+" affordance to promote this property to a table column. Omit when not applicable. */
  onPromote?: () => void;
  /** Whether this property is already promoted — disables the "+" and shows it as done. */
  promoted?: boolean;
}

export function DlRow({ label, children, copyValue, onPromote, promoted }: DlRowProps) {
  return (
    <div className="contents">
      <dt className="break-all font-bold text-[var(--muted)]">{label}</dt>
      <dd className="group m-0 flex min-w-0 items-start gap-1 break-all text-[var(--text)]">
        {children}
        {copyValue !== undefined && <CopyButton value={copyValue} size="xs" />}
        {onPromote && (
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              if (!promoted) onPromote();
              e.currentTarget.blur();
            }}
            disabled={promoted}
            title={promoted ? `${label} is already a column` : `Add ${label} as a column`}
            aria-label={promoted ? `${label} is already a column` : `Add ${label} as a column`}
            className="inline-flex shrink-0 items-center justify-center text-[var(--muted)] outline-none transition-opacity opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 focus-visible:opacity-100 focus-visible:ring-1 focus-visible:ring-[var(--focus-ring)] enabled:hover:text-[var(--brand)] disabled:opacity-40"
          >
            <Plus className="size-3" />
          </button>
        )}
      </dd>
    </div>
  );
}
