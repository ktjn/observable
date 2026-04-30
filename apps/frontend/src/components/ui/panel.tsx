import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export interface PanelProps extends HTMLAttributes<HTMLElement> {
  title?: string;
  eyebrow?: string;
  actions?: ReactNode;
  children: ReactNode;
}

export function Panel({
  title,
  eyebrow,
  actions,
  children,
  className,
  ...props
}: PanelProps) {
  return (
    <section className={cn("modern-panel overflow-hidden", className)} {...props}>
      {(title || eyebrow || actions) && (
        <div className="modern-panel-header">
          <div>
            {eyebrow && <div className="field-label">{eyebrow}</div>}
            {title && <h2 className="m-0 text-[13px] font-semibold text-[var(--text-strong)]">{title}</h2>}
          </div>
          {actions && <div className="modern-toolbar">{actions}</div>}
        </div>
      )}
      <div className="modern-panel-body">{children}</div>
    </section>
  );
}
