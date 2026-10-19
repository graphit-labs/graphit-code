import { MarkdownContent } from "@/components/wiki/WikiMarkdown";
import { StyledSelect } from "@/components/shared/StyledSelect";
import { useEffect, useState, useId } from "react";
import { ModalPortal } from "@/components/shared/ModalPortal";
import { FactList } from "@/components/shared/EngineeringUI";
import { bumpPatch } from "@/lib/utils";
import type { InstalledArtifact } from "@/api/hub";

const TYPES = [
  "agent",
  "rule",
  "skill",
  "command",
  "knowledge",
  "ast",
  "mcp",
  "power",
  "language",
  "framework",
];

interface Dep {
  type: string;
  id: string;
  version: string;
}

interface SubmitModalProps {
  open: boolean;
  artifact: InstalledArtifact | null;
  activeProjectId: string;
  gitAuthor: string;
  onSubmit: (payload: Record<string, unknown>) => Promise<void>;
  onClose: () => void;
}

export function SubmitModal({
  open,
  artifact,
  activeProjectId: _activeProjectId,
  gitAuthor,
  onSubmit,
  onClose,
}: SubmitModalProps) {
  const [step, setStep] = useState(0);
  const titleId = useId();
  const [name, setName] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState("");
  const [author, setAuthor] = useState("");
  const [deps, setDeps] = useState<Dep[]>([]);
  const [loading, setLoading] = useState(false);

  const isUpdate = !!artifact?.published;
  useEffect(() => {
    if (!open || !artifact) return;
    queueMicrotask(() => {
      setStep(0);
      setName(artifact.registry_name || artifact.local_id || "");
      setVersion(
        isUpdate ? bumpPatch(artifact.registry_version || "1.0.0") : "1.0.0",
      );
      setDescription(artifact.registry_description || "");
      setTags((artifact.registry_tags || []).join(", "));
      setAuthor(artifact.registry_author || gitAuthor);
      setDeps((artifact.registry_dependencies || []).map((d) => ({ ...d })));
    });

    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, artifact]);

  if (!open || !artifact) return null;

  const handleSubmit = async () => {
    setLoading(true);
    try {
      await onSubmit({
        id: artifact.local_id,
        type: artifact.type,
        version,
        name,
        description,
        tags,
        author: author || undefined,
        path: artifact.path || undefined,
        dependencies: deps.filter((d) => d.id),
      });
    } finally {
      setLoading(false);
    }
  };

  const addDep = () =>
    setDeps((d) => [...d, { type: "", id: "", version: "latest" }]);
  const removeDep = (i: number) =>
    setDeps((d) => d.filter((_, idx) => idx !== i));
  const updateDep = (i: number, field: keyof Dep, value: string) =>
    setDeps((d) =>
      d.map((dep, idx) => (idx === i ? { ...dep, [field]: value } : dep)),
    );

  return (
    <ModalPortal onClose={onClose}>
      <div className="work-modal-backdrop">
        <section
          className="work-dialog"
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
        >
          <header>
            <div>
              <p className="work-eyebrow">
                {isUpdate ? "New version" : "Publication"} ·{" "}
                {step === 0 ? "Describe" : "Review"}
              </p>
              <h2 id={titleId}>{artifact.local_id}</h2>
            </div>
            <button className="work-button" onClick={onClose}>
              Close
            </button>
          </header>
          <div className="work-dialog-body work-form">
            {step === 0 ? (
              <>
                <div className="work-form-grid">
                  <label className="work-field">
                    <span>Display name</span>
                    <input
                      value={name}
                      onChange={(e) => setName(e.target.value)}
                    />
                  </label>
                  <label className="work-field">
                    <span>Version</span>
                    <input
                      value={version}
                      onChange={(e) => setVersion(e.target.value)}
                    />
                  </label>
                </div>
                <label className="work-field">
                  <span>Description</span>
                  <textarea
                    rows={3}
                    placeholder="Purpose, scope and expected use. Markdown supported."
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                  />
                </label>
                <label className="work-field">
                  <span>Tags (comma-separated)</span>
                  <input
                    value={tags}
                    onChange={(e) => setTags(e.target.value)}
                  />
                </label>
                <label className="work-field">
                  <span>Author</span>
                  <input
                    value={author}
                    onChange={(e) => setAuthor(e.target.value)}
                  />
                </label>
                <fieldset>
                  <legend>Dependencies</legend>
                  {deps.map((dep, i) => (
                    <div key={i} className="work-form-grid work-panel">
                      <label className="work-field">
                        <span>Dependency {i + 1} type</span>
                        <StyledSelect
                          value={dep.type}
                          onChange={(e) => updateDep(i, "type", e.target.value)}
                        >
                          <option value="">Choose type</option>
                          {TYPES.map((t) => (
                            <option key={t}>{t}</option>
                          ))}
                        </StyledSelect>
                      </label>
                      <label className="work-field">
                        <span>Dependency {i + 1} identifier</span>
                        <input
                          value={dep.id}
                          onChange={(e) => updateDep(i, "id", e.target.value)}
                        />
                      </label>
                      <label className="work-field">
                        <span>Dependency {i + 1} version</span>
                        <input
                          value={dep.version}
                          onChange={(e) =>
                            updateDep(i, "version", e.target.value)
                          }
                        />
                      </label>
                      <button
                        className="work-button danger"
                        onClick={() => removeDep(i)}
                      >
                        Remove dependency {i + 1}
                      </button>
                    </div>
                  ))}
                  <button className="work-button" onClick={addDep}>
                    Add dependency
                  </button>
                </fieldset>
              </>
            ) : (
              <>
                <p>
                  Review the version and reusable contract before publishing to
                  the registry.
                </p>
                <FactList
                  items={[
                    ["Artifact", artifact.local_id],
                    ["Type", artifact.type],
                    ["Name", name],
                    ["Version", version],
                    ["Author", author],
                    ["Tags", tags],
                  ]}
                />
                <h3>Description</h3>
                {description ? <div className="markdown-preview"><MarkdownContent content={description} /></div> : <p>No description provided.</p>}
                <h3>Dependencies</h3>
                {deps
                  .filter((d) => d.id)
                  .map((d, i) => (
                    <p key={i}>
                      <code>
                        {d.type} · {d.id}@{d.version}
                      </code>
                    </p>
                  ))}
                {!deps.some((d) => d.id) && <p>No dependencies.</p>}
              </>
            )}
          </div>
          <footer>
            <button
              className="work-button"
              onClick={step === 0 ? onClose : () => setStep(0)}
            >
              {step === 0 ? "Cancel" : "Back to metadata"}
            </button>
            {step === 0 ? (
              <button
                className="work-button primary"
                disabled={!name.trim() || !version.trim()}
                onClick={() => setStep(1)}
              >
                Review publication
              </button>
            ) : (
              <button
                className="work-button primary"
                disabled={loading}
                onClick={handleSubmit}
              >
                {loading
                  ? "Publishing…"
                  : isUpdate
                    ? "Publish new version"
                    : "Publish artifact"}
              </button>
            )}
          </footer>
        </section>
      </div>
    </ModalPortal>
  );
}
