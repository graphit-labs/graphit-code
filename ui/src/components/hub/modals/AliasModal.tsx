import { useState, useId } from "react";
import { ModalPortal } from "@/components/shared/ModalPortal";
interface AliasModalProps {
  open: boolean;
  artifactId: string;
  requireAlias: boolean;
  onConfirm: (alias: string | null) => void;
  onCancel: () => void;
}
export function AliasModal(props: AliasModalProps) {
  return props.open ? <AliasEditor key={props.artifactId} {...props} /> : null;
}
function AliasEditor({
  artifactId,
  requireAlias,
  onConfirm,
  onCancel,
}: AliasModalProps) {
  const [alias, setAlias] = useState("");
  const id = useId();
  return (
    <ModalPortal onClose={onCancel}>
      <div className="work-modal-backdrop">
        <section
          className="work-dialog"
          style={{ maxWidth: 560 }}
          role="dialog"
          aria-modal="true"
          aria-labelledby={id}
        >
          <header>
            <div>
              <p className="work-eyebrow">Install context</p>
              <h2 id={id}>Choose a local name</h2>
            </div>
            <button className="work-button" onClick={onCancel}>
              Close
            </button>
          </header>
          <div className="work-dialog-body work-form">
            <code>{artifactId}</code>
            <p>
              {requireAlias
                ? "An installed artifact already uses this name. Choose a unique alias to keep both contexts distinguishable."
                : "An alias makes this artifact easier to address in the selected project."}
            </p>
            <label className="work-field">
              <span>
                Local alias {requireAlias ? "(required)" : "(optional)"}
              </span>
              <input
                autoFocus
                value={alias}
                onChange={(e) => setAlias(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && (!requireAlias || alias.trim()))
                    onConfirm(alias.trim() || null);
                }}
                placeholder="team-engineering"
              />
            </label>
          </div>
          <footer>
            <button className="work-button" onClick={onCancel}>
              Cancel
            </button>
            <button
              className="work-button primary"
              disabled={requireAlias && !alias.trim()}
              onClick={() => onConfirm(alias.trim() || null)}
            >
              Install artifact
            </button>
          </footer>
        </section>
      </div>
    </ModalPortal>
  );
}
