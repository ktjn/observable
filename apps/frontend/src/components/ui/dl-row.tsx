import type { ReactNode } from "react";
import { CopyButton } from "./copy-button";

export interface DlRowProps {
  label: string;
  children: ReactNode;
  copyValue?: string;
}

export function DlRow({ label, children, copyValue }: DlRowProps) {
  return (
    <div className="contents">
      <dt className="break-all font-bold text-[var(--muted)]">{label}</dt>
      <dd className="group m-0 flex min-w-0 items-start gap-1 break-all text-[var(--text)]">
        {children}
        {copyValue !== undefined && <CopyButton value={copyValue} size="xs" />}
      </dd>
    </div>
  );
}
