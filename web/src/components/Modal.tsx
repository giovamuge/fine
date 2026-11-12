// Accessible dialog on top of the native <dialog>: showModal() gives focus trapping,
// Escape, inert background and aria-modal for free. On phones it opens as a bottom sheet.
import { useEffect, useId, useRef, type ReactNode } from "react";
import { Icon } from "./Icon";

export function Modal({
  open,
  onClose,
  title,
  children,
  sheet = true,
  closeLabel = "Chiudi",
  initialFocus,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  sheet?: boolean;
  closeLabel?: string;
  /** Selector of the element to focus on open (defaults to the first focusable). */
  initialFocus?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (open && !el.open) {
      el.showModal();
      const target = initialFocus ? el.querySelector<HTMLElement>(initialFocus) : null;
      (target ?? el.querySelector<HTMLElement>("input, select, textarea, button:not(.close)"))?.focus();
    } else if (!open && el.open) {
      el.close();
    }
  }, [open, initialFocus]);

  // Closing the dialog through Escape must propagate to React state.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const onCancel = (e: Event) => {
      e.preventDefault();
      onClose();
    };
    el.addEventListener("cancel", onCancel);
    return () => el.removeEventListener("cancel", onCancel);
  }, [onClose]);

  return (
    <dialog
      ref={ref}
      className={`modal${sheet ? " sheet" : ""}`}
      aria-labelledby={titleId}
      onClick={(e) => {
        // A click on the backdrop (outside the panel) closes.
        if (e.target === ref.current) onClose();
      }}
    >
      {open && (
        <div className="modal-body">
          <h2 className="title" id={titleId}>
            {title}
          </h2>
          <button type="button" className="btn ghost icon close" onClick={onClose} aria-label={closeLabel}>
            <Icon name="x" />
          </button>
          {children}
        </div>
      )}
    </dialog>
  );
}

/** Yes/no question with an explicit, optionally destructive, confirmation. */
export function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  children,
  confirmLabel = "Conferma",
  cancelLabel = "Annulla",
  danger = false,
  busy = false,
}: {
  open: boolean;
  onClose: () => void;
  onConfirm: () => void;
  title: string;
  children: ReactNode;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  busy?: boolean;
}) {
  return (
    <Modal open={open} onClose={onClose} title={title} initialFocus=".actions .cancel">
      <div className="stack">{children}</div>
      <div className="actions">
        <button type="button" className="btn cancel" onClick={onClose} disabled={busy}>
          {cancelLabel}
        </button>
        <button type="button" className={`btn ${danger ? "danger solid" : "primary"}`} onClick={onConfirm} disabled={busy}>
          {busy && <Icon name="sync" size={16} className="spin" />}
          {confirmLabel}
        </button>
      </div>
    </Modal>
  );
}
