import { useEffect, useId, useRef } from "react";
import { Button } from "./button";

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  consequences?: string;
  elevatedContext?: { scope?: string; justification?: string } | null;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "primary" | "destructive";
  isPending?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Accessible confirmation dialog for high-impact or irreversible actions (OBS-24-03).
 *
 * Focuses Cancel by default so an unintended Enter or double-click cannot complete
 * the operation without deliberate confirmation.
 */
export function ConfirmDialog({
  open,
  title,
  description,
  consequences,
  elevatedContext,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  variant = "destructive",
  isPending = false,
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const titleId = useId();
  const descId = useId();
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    cancelRef.current?.focus();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !isPending) {
        onCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, isPending, onCancel]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
      role="presentation"
      onClick={(e) => {
        if (e.target === e.currentTarget && !isPending) onCancel();
      }}
    >
      <div
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descId}
        className="modern-panel w-full max-w-md grid gap-4 border border-[var(--border)] bg-[var(--panel)] p-6 shadow-xl"
      >
        <div className="grid gap-2">
          <h2 id={titleId} className="text-lg font-bold text-[var(--text)]">
            {title}
          </h2>
          <p id={descId} className="m-0 text-sm text-[var(--muted)]">
            {description}
          </p>
          {consequences && (
            <p className="m-0 rounded border border-[var(--warn)] bg-[var(--warn-bg,transparent)] px-3 py-2 text-xs text-[var(--text)]">
              <strong>Consequence:</strong> {consequences}
            </p>
          )}
          {elevatedContext && (
            <p
              data-testid="elevated-action-disclosure"
              className="m-0 rounded border border-[var(--bad)] bg-[var(--bad-bg,transparent)] px-3 py-2 text-xs text-[var(--text)]"
            >
              <strong>Elevated access active:</strong> This action runs under elevated permissions
              {elevatedContext.scope ? ` (${elevatedContext.scope})` : ""} and will be recorded with
              actor, justification{elevatedContext.justification ? ` ("${elevatedContext.justification}")` : ""},
              and elevation grant.
            </p>
          )}
        </div>
        <div className="flex flex-wrap justify-end gap-2">
          <Button
            ref={cancelRef}
            type="button"
            variant="secondary"
            disabled={isPending}
            onClick={onCancel}
          >
            {cancelLabel}
          </Button>
          <Button
            type="button"
            variant={variant}
            disabled={isPending}
            onClick={onConfirm}
          >
            {isPending ? "Working…" : confirmLabel}
          </Button>
        </div>
      </div>
    </div>
  );
}
