import { StyledSelect } from "@/components/shared/StyledSelect";
import {
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { useState, useRef, useCallback, useEffect } from "react";
import { useAppStore } from "@/store/appStore";
import { showToast } from "@/hooks/useToast";
import { hubApi } from "@/api/hub";
import { CloudUpload, Plus, Trash2, Wand2 } from "lucide-react";
import { cn } from "@/lib/utils";

const TYPES = [
  "rule",
  "skill",
  "agent",
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

function uploadExtension(type: string) {
  if (type === "ast") return ".ast";
  if (type === "knowledge") return ".knowledge";
  return ".zip";
}

export default function UploadPage() {
  const { activeProjectDir, activeAgent } = useAppStore();
  return (
    <PublicationWorkspace
      key={JSON.stringify([activeProjectDir, activeAgent])}
    />
  );
}
function PublicationWorkspace() {
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const { webMode, activeAgent, activeProjectDir } = useAppStore();

  const [step, setStep] = useState(0);
  const stepPanel = useRef<HTMLElement>(null);
  useEffect(() => {
    const heading = stepPanel.current?.querySelector("h2");
    if (heading) {
      heading.tabIndex = -1;
      heading.focus({ preventScroll: true });
      heading.scrollIntoView({ block: "start" });
    }
  }, [step]);
  const [published, setPublished] = useState("");
  const [file, setFile] = useState<File | null>(null);
  const [artifactId, setArtifactId] = useState("");
  const [name, setName] = useState("");
  const [version, setVersion] = useState("1.0.0");
  const [type, setType] = useState("");
  const [description, setDescription] = useState("");
  const [tags, setTags] = useState("");
  const [author, setAuthor] = useState("");
  const [deps, setDeps] = useState<Dep[]>([]);
  const [loading, setLoading] = useState(false);
  const [dragging, setDragging] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const acceptedExtension = uploadExtension(type);

  useEffect(() => {
    if (!webMode) {
      hubApi
        .getGitAuthor()
        .then((d) => setAuthor(d.author))
        .catch(() => {});
    }
  }, [webMode]);

  const handleFileDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragging(false);
      const f = e.dataTransfer.files[0];
      if (f && f.name.toLowerCase().endsWith(acceptedExtension)) setFile(f);
      else
        showToast(
          `Artifact type ${type || "selected"} requires a ${acceptedExtension} file`,
          "error",
        );
    },
    [acceptedExtension, type],
  );

  const handleFileSelect = (e: React.ChangeEvent<HTMLInputElement>) => {
    const f = e.target.files?.[0];
    if (!f) return;
    if (!f.name.toLowerCase().endsWith(acceptedExtension)) {
      showToast(
        `Artifact type ${type || "selected"} requires a ${acceptedExtension} file`,
        "error",
      );
      e.target.value = "";
      return;
    }
    setFile(f);
  };

  const addDep = () =>
    setDeps((d) => [...d, { type: "", id: "", version: "latest" }]);
  const removeDep = (i: number) =>
    setDeps((d) => d.filter((_, idx) => idx !== i));
  const updateDep = (i: number, field: keyof Dep, value: string) =>
    setDeps((d) =>
      d.map((dep, idx) => (idx === i ? { ...dep, [field]: value } : dep)),
    );

  const handleSubmit = async () => {
    const isPower = type === "power";
    if (!isPower && !file) {
      showToast(`Please select a ${acceptedExtension} file`, "error");
      return;
    }
    if (!artifactId || !type) {
      showToast("Artifact ID and type are required", "error");
      return;
    }
    setLoading(true);
    try {
      const formData = new FormData();
      if (file) formData.append("file", file);
      formData.append("id", artifactId);
      formData.append("type", type);
      formData.append("version", version);
      formData.append("name", name);
      formData.append("description", description);
      formData.append("tags", tags);
      formData.append("author", author);
      formData.append("agent", activeAgent);
      formData.append("dependencies", JSON.stringify(deps.filter((d) => d.id)));
      if (activeProjectDir) formData.append("project_dir", activeProjectDir);

      const result = await hubApi.upload(formData);
      if (!mounted.current) return;
      if (result.success) {
        showToast("Upload successful!", "success");
        setPublished(artifactId + "@" + version);
        setStep(0);
        setFile(null);
        setArtifactId("");
        setName("");
        setDescription("");
        setTags("");
        setDeps([]);
      } else {
        showToast(`Upload failed: ${result.error ?? "unknown"}`, "error");
      }
    } catch {
      showToast("Upload failed", "error");
    } finally {
      setLoading(false);
    }
  };

  const readyPackage = !!type && (type === "power" || !!file);
  const readyMetadata = !!artifactId.trim() && !!version.trim();
  return (
    <WorkPage>
      <WorkHeader
        title="Publish an artifact"
        description="Package reusable engineering context, describe its contract and review the publication."
      />
      {published && (
        <WorkNotice tone="success" title="Artifact published">
          {published}
        </WorkNotice>
      )}
      <ol className="publish-steps" aria-label="Publication steps">
        {["Package", "Metadata & dependencies", "Review & publish"].map(
          (label, i) => (
            <li key={label} aria-current={step === i ? "step" : undefined}>
              <button disabled={i > step} onClick={() => setStep(i)}>
                <span>{i + 1}</span>
                {label}
              </button>
            </li>
          ),
        )}
      </ol>
      <div className="publish-layout">
        <section className="work-panel" ref={stepPanel}>
          {step === 0 && (
            <WorkSection
              title="Choose what you are sharing"
              description="Artifact type determines the required package format."
            >
              <div className="work-form">
                <label className="work-field">
                  <span>Artifact type</span>
                  <StyledSelect
                    value={type}
                    onChange={(e) => {
                      setType(e.target.value);
                      setFile(null);
                      if (fileRef.current) fileRef.current.value = "";
                    }}
                  >
                    <option value="">Select a type</option>
                    {TYPES.map((t) => (
                      <option key={t}>{t}</option>
                    ))}
                  </StyledSelect>
                </label>
                {type === "power" ? (
                  <WorkNotice title="A virtual package">
                    Powers group dependencies and require no file.
                  </WorkNotice>
                ) : (
                  <div
                    className={"package-drop " + (dragging ? "dragging" : "")}
                    onDragOver={(e) => {
                      e.preventDefault();
                      setDragging(true);
                    }}
                    onDragLeave={() => setDragging(false)}
                    onDrop={handleFileDrop}
                  >
                    <label className="work-field">
                      <span>Package file · {acceptedExtension}</span>
                      <input
                        ref={fileRef}
                        type="file"
                        accept={acceptedExtension}
                        onChange={handleFileSelect}
                        disabled={!type}
                      />
                      <small>
                        {file
                          ? file.name +
                            " · " +
                            (file.size / 1024).toFixed(1) +
                            " KB"
                          : "Choose a file or drop it here."}
                      </small>
                    </label>
                  </div>
                )}
                {["ast", "knowledge"].includes(type) && (
                  <WorkNotice title="Use an exported package">
                    <code>graphit {type} export --format package</code>
                    <p>
                      The server validates the package envelope and native
                      store.
                    </p>
                  </WorkNotice>
                )}
                <div className="work-actions">
                  <button
                    className="work-button primary"
                    disabled={!readyPackage}
                    onClick={() => setStep(1)}
                  >
                    Describe artifact
                  </button>
                </div>
              </div>
            </WorkSection>
          )}
          {step === 1 && (
            <WorkSection
              title="Make the artifact understandable"
              description="Use a stable identifier and describe when this context should be applied."
            >
              <div className="work-form">
                <div className="work-two-columns">
                  <label className="work-field">
                    <span>Artifact ID</span>
                    <input
                      value={artifactId}
                      onChange={(e) => setArtifactId(e.target.value)}
                      placeholder="delivery-evidence"
                    />
                  </label>
                  <label className="work-field">
                    <span>Version</span>
                    <input
                      value={version}
                      onChange={(e) => setVersion(e.target.value)}
                      placeholder="1.0.0"
                    />
                  </label>
                </div>
                <label className="work-field">
                  <span>Display name</span>
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    placeholder="Delivery evidence"
                  />
                </label>
                <label className="work-field">
                  <span>Description</span>
                  <textarea
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    placeholder="Purpose, scope and expected use"
                  />
                </label>
                <div className="work-two-columns">
                  <label className="work-field">
                    <span>Tags, comma-separated</span>
                    <input
                      value={tags}
                      onChange={(e) => setTags(e.target.value)}
                    />
                  </label>
                  <label className="work-field">
                    <span>Author</span>
                    <input
                      value={author}
                      disabled={webMode}
                      onChange={(e) => setAuthor(e.target.value)}
                    />
                  </label>
                </div>
                <WorkSection
                  title="Dependencies"
                  description="Declare the other artifacts this package needs."
                  actions={
                    <button className="work-button" onClick={addDep}>
                      Add dependency
                    </button>
                  }
                >
                  {deps.map((d, i) => (
                    <div className="dependency-row" key={i}>
                      <label className="work-field">
                        <span>Type</span>
                        <StyledSelect
                          value={d.type}
                          onChange={(e) => updateDep(i, "type", e.target.value)}
                        >
                          <option value="">Select</option>
                          {TYPES.map((t) => (
                            <option key={t}>{t}</option>
                          ))}
                        </StyledSelect>
                      </label>
                      <label className="work-field">
                        <span>Identifier</span>
                        <input
                          value={d.id}
                          onChange={(e) => updateDep(i, "id", e.target.value)}
                        />
                      </label>
                      <label className="work-field">
                        <span>Version</span>
                        <input
                          value={d.version}
                          onChange={(e) =>
                            updateDep(i, "version", e.target.value)
                          }
                        />
                      </label>
                      <button
                        className="work-button danger"
                        onClick={() => removeDep(i)}
                        aria-label={"Remove dependency " + (i + 1)}
                      >
                        Remove
                      </button>
                    </div>
                  ))}
                </WorkSection>
                <div className="work-actions">
                  <button className="work-button" onClick={() => setStep(0)}>
                    Back
                  </button>
                  <button
                    className="work-button primary"
                    disabled={!readyMetadata}
                    onClick={() => setStep(2)}
                  >
                    Review publication
                  </button>
                </div>
              </div>
            </WorkSection>
          )}
          {step === 2 && (
            <WorkSection
              title="Review the publication"
              description="This action publishes under the active project's publisher identity."
            >
              <FactList
                items={[
                  ["Artifact", artifactId],
                  ["Name", name],
                  ["Type", type],
                  ["Version", version],
                  [
                    "Package",
                    type === "power" ? "Virtual package" : file?.name,
                  ],
                  ["Description", description],
                  ["Tags", tags],
                  ["Author", author],
                  [
                    "Dependencies",
                    deps
                      .filter((d) => d.id)
                      .map((d) => d.type + "/" + d.id + "@" + d.version)
                      .join(", "),
                  ],
                ]}
              />
              <div className="work-actions mt-6">
                <button
                  className="work-button"
                  disabled={loading}
                  onClick={() => setStep(1)}
                >
                  Edit metadata
                </button>
                <button
                  className="work-button primary"
                  disabled={loading || !readyPackage || !readyMetadata}
                  onClick={() => void handleSubmit()}
                >
                  {loading ? "Publishing…" : "Upload & Publish"}
                </button>
              </div>
            </WorkSection>
          )}
        </section>
        <aside>
          <WorkSection title="Publication context">
            <FactList
              items={[
                ["Project", activeProjectDir || "Current project"],
                ["Agent", activeAgent],
                [
                  "Package format",
                  type === "power" ? "Virtual" : acceptedExtension,
                ],
              ]}
            />
          </WorkSection>
          <WorkNotice title="Reusable engineering context">
            A useful artifact makes its purpose, dependencies and version
            explicit.
          </WorkNotice>
        </aside>
      </div>
    </WorkPage>
  );
}
