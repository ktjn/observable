import { useRef } from "react";
import type { RefObject } from "react";
import { AlertDialog } from "@ktjn/designed-react";
import { Button } from "./button";

// Designed's AlertDialogContentProps doesn't declare Base UI's initialFocus/finalFocus,
// but forwards unknown props to the underlying Popup, which does read them.
type AlertDialogContentWithFocusProps = { initialFocus?: RefObject<HTMLElement | null> };

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
 * the operation without deliberate confirmation. Backed by Designed's AlertDialog
 * (Base UI), which disables pointer-outside dismissal for alertdialogs by design —
 * only Cancel, Confirm or Escape (gated below by isPending) close it.
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
  const cancelRef = useRef<HTMLButtonElement>(null);

  return (
    <AlertDialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !isPending) onCancel();
      }}
    >
      <AlertDialog.Content
        className="grid gap-4"
        {...({ initialFocus: cancelRef } satisfies AlertDialogContentWithFocusProps)}
      >
        <div className="grid gap-2">
          <AlertDialog.Title className="text-lg font-bold">{title}</AlertDialog.Title>
          <AlertDialog.Description className="m-0 text-sm text-[var(--muted)]">
            {description}
          </AlertDialog.Description>
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
      </AlertDialog.Content>
    </AlertDialog>
  );
}
