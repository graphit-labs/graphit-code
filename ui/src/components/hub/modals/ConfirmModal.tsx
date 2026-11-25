import { useId } from "react";
import { ModalPortal } from "@/components/shared/ModalPortal";
interface ConfirmModalProps {
  open: boolean;
  title: string;
  message: string;
  warning?: string;
  confirmLabel?: string;
  variant?: "danger" | "default";
  onConfirm: () => void;
  onCancel: () => void;
}
export function ConfirmModal({
  open,
  title,
  message,
  warning = "This action cannot be undone.",
  confirmLabel = "Confirm",
  variant = "danger",
  onConfirm,
  onCancel,
}: ConfirmModalProps) {
  const id = useId();
  if (!open) return null;
  return (
    <ModalPortal onClose={onCancel}>
      <div className="work-modal-backdrop">
        <section
          role="dialog"
          aria-modal="true"
          aria-labelledby={id}
          aria-describedby={id + "-message"}
          className="work-dialog"
          style={{ maxWidth: 520 }}
        >
          <header>
            <div>
              <p className="work-eyebrow">Review action</p>
              <h2 id={id}>{title}</h2>
            </div>
            <button
              className="work-button"
              onClick={onCancel}
              aria-label="Close confirmation"
            >
              Close
            </button>
          </header>
          <div className="work-dialog-body">
            <p id={id + "-message"}>{message}</p>
            {warning && <p className="work-notice mt-4">{warning}</p>}
          </div>
          <footer>
            <button className="work-button" onClick={onCancel}>
              Cancel
            </button>
            <button
              className={
                "work-button " + (variant === "danger" ? "danger" : "primary")
              }
              onClick={onConfirm}
            >
              {confirmLabel}
            </button>
          </footer>
        </section>
      </div>
    </ModalPortal>
  );
}
