import { RecordReferences } from "@/components/shared/RecordReferences";
import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
  WorkBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkEmpty,
  WorkTabs,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { ModalPortal } from "@/components/shared/ModalPortal";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  AlertTriangle,
  ArrowLeft,
  Brain,
  CheckCircle2,
  Edit3,
  FileClock,
  Fingerprint,
  Flag,
  Plus,
  RefreshCw,
  Save,
  Search,
  ShieldCheck,
  Tag,
  Trash2,
  X,
} from "lucide-react";

import {
  memoryApi,
  sortMemoryCatalogItems,
  type MemoryCatalog,
  type MemoryCatalogItem,
  type MemoryScope,
  type MemoryTrace,
  type MemoryUpdate,
  type MemoryVersion,
  type MemoryWrite,
} from "@/api/memory";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { showToast } from "@/hooks/useToast";
import { projectRequestScope } from "@/lib/projectScope";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";
import { MemoryMarkdown } from "./MemoryMarkdown";

const memoryTypes = [
  "fact",
  "decision",
  "convention",
  "correction",
  "tension",
  "skill",
];

function shortDate(value?: string) {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function TypeBadge({ type }: { type: string }) {
  return <WorkBadge>{type || "untyped"}</WorkBadge>;
}

function MemoryFlags({ important, mandatory }: { important: boolean; mandatory: boolean }) {
  return <div className="flex flex-wrap items-center gap-1.5">
    {important && <WorkBadge tone="warning" title="Important"><Flag aria-hidden="true" />Important</WorkBadge>}
    {mandatory && <WorkBadge tone="info" title="Mandatory at session start"><ShieldCheck aria-hidden="true" />Mandatory</WorkBadge>}
  </div>;
}

function Meta({
  label,
  value,
  mono = false,
}: {
  label: string;
  value?: string | number;
  mono?: boolean;
}) {
  return (
    <div className="min-w-0 rounded-xl border border-border/35 bg-background/45 p-3">
      <p className="text-[9px] font-bold uppercase tracking-[0.16em] text-muted-foreground">
        {label}
      </p>
      <p
        className={cn(
          "mt-1 break-all text-xs font-semibold text-foreground",
          mono && "font-mono text-[11px]",
        )}
      >
        {value || "—"}
      </p>
    </div>
  );
}

interface MemoryFormProps {
  initial?: MemoryVersion;
  saving: boolean;
  onClose: () => void;
  onSave: (value: MemoryWrite | MemoryUpdate) => void;
}

function MemoryForm({ initial, saving, onClose, onSave }: MemoryFormProps) {
  const [title, setTitle] = useState(initial?.title ?? "");
  const [body, setBody] = useState(initial?.body ?? "");
  const [type, setType] = useState(initial?.type ?? "fact");
  const [tags, setTags] = useState(
    initial?.tags
      .filter((tag) => !["memory", initial.scope, initial.type].includes(tag))
      .join(", ") ?? "",
  );
  const [important, setImportant] = useState(initial?.important ?? false);
  const [mandatory, setMandatory] = useState(initial?.mandatory ?? false);
  const valid = title.trim() !== "" && body.trim() !== "";

  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    if (!valid) return;
    const value: MemoryWrite = {
      title: title.trim(),
      body: body.trim(),
      type,
      important,
      mandatory,
      ...(!initial
        ? {
            tags: tags
              .split(",")
              .map((tag) => tag.trim())
              .filter(Boolean),
          }
        : {}),
    };
    onSave(value);
  };

  return (
    <ModalPortal onClose={onClose}>
      <div className="work-modal-backdrop">
        <form
          role="dialog"
          aria-modal="true"
          aria-label={initial ? "Edit memory" : "Create memory"}
          onSubmit={submit}
          className="work-dialog"
        >
          <header>
            <div>
              <h2>
                {initial ? "Refine this memory" : "Capture a durable memory"}
              </h2>
              <p>
                Record what future work needs to know, with enough context to
                apply it correctly.
              </p>
            </div>
            <button
              className="work-button"
              type="button"
              aria-label="Close memory form"
              onClick={onClose}
            >
              Close
            </button>
          </header>
          <div className="work-editor-layout">
            <section className="work-form">
              <label className="work-field">
                <span>Title</span>
                <input
                  aria-label="Memory title"
                  autoFocus
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  placeholder="A concise statement of the lesson or decision"
                />
              </label>
              <label className="work-field">
                <span>Content</span>
                <textarea
                  aria-label="Memory content"
                  value={body}
                  onChange={(e) => setBody(e.target.value)}
                  rows={14}
                />
                <small>
                  Markdown supported. Include scope, rationale and the
                  circumstances where this applies.
                </small>
              </label>
            </section>
            <aside className="work-form">
              <label className="work-field">
                <span>Type</span>
                <StyledSelect
                  aria-label="Memory type"
                  value={type}
                  onChange={(e) => setType(e.target.value)}
                >
                  {memoryTypes.map((t) => (
                    <option key={t}>{t}</option>
                  ))}
                </StyledSelect>
              </label>
              {!initial && (
                <label className="work-field">
                  <span>Tags</span>
                  <input
                    aria-label="Memory tags"
                    value={tags}
                    onChange={(e) => setTags(e.target.value)}
                    placeholder="architecture, storage"
                  />
                </label>
              )}
              <label className="work-check">
                <input
                  aria-label="Important memory"
                  type="checkbox"
                  checked={important}
                  onChange={(e) => setImportant(e.target.checked)}
                />
                <span>
                  <strong>Important</strong>
                  <small>Promoted for retrieval.</small>
                </span>
              </label>
              <label className="work-check">
                <input
                  aria-label="Mandatory memory"
                  type="checkbox"
                  checked={mandatory}
                  onChange={(e) => setMandatory(e.target.checked)}
                />
                <span>
                  <strong>Mandatory</strong>
                  <small>Loaded in full at session start.</small>
                </span>
              </label>
              {initial && (
                <WorkNotice title="A traceable revision">
                  Classification tags are preserved during edits. Content
                  changes retain their history.
                </WorkNotice>
              )}
            </aside>
          </div>
          <footer>
            <button className="work-button" type="button" onClick={onClose}>
              Cancel
            </button>
            <button className="work-button primary" disabled={!valid || saving}>
              {saving ? "Saving…" : initial ? "Save revision" : "Create memory"}
            </button>
          </footer>
        </form>
      </div>
    </ModalPortal>
  );
}

function MemoryDetail({
  trace,
  onBack,
  onEdit,
  onRemove,
}: {
  trace: MemoryTrace;
  onBack: () => void;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const [selectedKey, setSelectedKey] = useState(
    trace.current?.key ?? trace.revisions.at(-1)?.key ?? "",
  );
  const versions = [
    ...(trace.current ? [trace.current] : []),
    ...[...trace.revisions]
      .reverse()
      .filter((version) => version.key !== trace.current?.key),
  ];
  const selected =
    versions.find((version) => version.key === selectedKey) ?? versions[0];
  if (!selected)
    return (
      <EmptyState
        icon={AlertTriangle}
        title="Trace unavailable"
        description="No authoritative versions were returned for this memory."
      />
    );

  return (
    <article className="work-dossier" aria-label="Memory record">
      <header className="dossier-header">
        <div>
          <div className="dossier-status">
            <TypeBadge type={selected.type} />
            <WorkBadge>{selected.status}</WorkBadge>
            <MemoryFlags
              important={selected.important}
              mandatory={selected.mandatory}
            />
          </div>
          <h2>{selected.title}</h2>
          <small>
            <code>{selected.id}</code> · revision {selected.revision}
          </small>
        </div>
        <div className="work-actions">
          <button className="work-button" onClick={onBack}>
            Memories
          </button>
          {trace.current && (
            <>
              <button className="work-button" onClick={onEdit}>
                Edit
              </button>
              <button className="work-button danger" onClick={onRemove}>
                Remove
              </button>
            </>
          )}
        </div>
      </header>
      <div className="memory-revisions">
        <h3>Revision chain</h3>
        <div role="group" aria-label="Memory revisions">
          {versions.map((v) => (
            <button
              key={v.key}
              className="work-button"
              aria-pressed={v.key === selected.key}
              onClick={() => setSelectedKey(v.key)}
            >
              Revision {v.revision}
              <small>{v.status}</small>
            </button>
          ))}
        </div>
      </div>
      <div className="dossier-layout">
        <div className="dossier-content">
          {selected.status !== "current" && (
            <WorkNotice title="Historical revision">
              You are reading a previous version. Editing updates the current
              memory.
            </WorkNotice>
          )}
          <RecordReferences kind="memory" id={trace.memory_id} scope={selected.scope} />
          <WorkSection title="Recorded guidance">
            <MemoryMarkdown content={selected.body} title={selected.title} />
          </WorkSection>
          {selected.tags.length > 0 && (
            <WorkSection title="Classification">
              <div className="work-actions">
                {selected.tags.map((t) => (
                  <WorkBadge key={t}>
                    {t}
                  </WorkBadge>
                ))}
              </div>
            </WorkSection>
          )}
        </div>
        <aside className="dossier-context">
          <h3>Authoritative metadata</h3>
          <FactList
            items={[
              ["Scope", selected.scope + " / " + selected.scope_id],
              ["Associated project", selected.project_id],
              ["Updated by", selected.updated_by],
              ["Created", shortDate(selected.created_at)],
              ["Updated", shortDate(selected.updated_at)],
              ["Content hash", selected.content_hash],
              ["Previous", selected.previous],
              ["Next", selected.next],
              ["Revision address", selected.revision_id || selected.key],
            ]}
          />
        </aside>
      </div>
    </article>
  );
}

export default function MemoryExplorerPage() {
  const navigate = useNavigate();
  const { scopeId, memoryId } = useParams<{
    scopeId?: string;
    memoryId?: string;
  }>();
  const scope: MemoryScope = scopeId === "user" ? "user" : "project";
  const { activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId, projectName } = useAppStore();
  const { key: projectKey, projectDir, projectId: selectedProjectId } = projectRequestScope({
    activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId,
  });
  const projectId = scope === "project" ? selectedProjectId : undefined;
  const [catalog, setCatalog] = useState<MemoryCatalog>({
    results: [],
    total: 0,
    types: [],
    tags: [],
  });
  const [trace, setTrace] = useState<MemoryTrace | null>(null);
  const [query, setQuery] = useState("");
  const [type, setType] = useState("all");
  const [tag, setTag] = useState("all");
  const [important, setImportant] = useState("all");
  const [mandatory, setMandatory] = useState("all");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [form, setForm] = useState<"create" | "edit" | null>(null);
  const catalogRequest = useRef(0);
  const detailRequest = useRef(0);
  const previousProject = useRef(projectKey);
  const detailTrace = trace && trace.memory_id === memoryId ? trace : null;

  const loadCatalog = useCallback(async () => {
    const request = ++catalogRequest.current;
    setLoading(true);
    try {
      const result = await memoryApi.list({
        projectDir,
        ...(projectId ? { projectId } : {}),
        scope,
        query: query.trim() || undefined,
        type,
        tag,
        important,
        mandatory,
      });
      if (request !== catalogRequest.current) return;
      const ordered = {
        ...result,
        results: sortMemoryCatalogItems(result.results),
      };
      setCatalog(ordered);
      if (!memoryId && ordered.results[0])
        navigate(
          `/memory/explorer/${scope}/${encodeURIComponent(ordered.results[0].id)}`,
          { replace: true },
        );
      window.document.title = `Graphit Memory — ${scope === "user" ? "User" : projectName || "Project"}`;
    } catch {
      if (request === catalogRequest.current) {
        setCatalog({ results: [], total: 0, types: [], tags: [] });
        showToast("Failed to load memories", "error");
      }
    } finally {
      if (request === catalogRequest.current) setLoading(false);
    }
  }, [
    projectDir,
    projectId,
    important,
    mandatory,
    memoryId,
    navigate,
    projectName,
    query,
    scope,
    tag,
    type,
  ]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadCatalog();
    }, 180);
    return () => window.clearTimeout(timer);
  }, [loadCatalog]);
  const loadTrace = useCallback(
    (id: string) => {
      const request = ++detailRequest.current;
      const requestTrace = projectId
        ? memoryApi.detail(projectDir, scope, id, projectId)
        : memoryApi.detail(projectDir, scope, id);
      return requestTrace
        .then((result) => {
          if (request === detailRequest.current) setTrace(result);
        })
        .catch(() => {
          if (request === detailRequest.current) {
            setTrace(null);
            showToast("Failed to load memory trace", "error");
          }
        });
    },
    [projectDir, projectId, scope],
  );
  useEffect(() => {
    if (!memoryId) {
      detailRequest.current += 1;
      return;
    }
    loadTrace(memoryId);
  }, [loadTrace, memoryId]);
  useEffect(() => {
    if (previousProject.current === projectKey) return;
    previousProject.current = projectKey;
    setTrace(null);
    navigate(`/memory/explorer/${scope}`, { replace: true });
  }, [projectKey, navigate, scope]);

  // Clicking the already selected row navigates to the URL it is already on, so the trace
  // effect cannot run: keep the rendered trace and only reload when nothing is shown, which
  // recovers from a trace request that failed earlier.
  const selectMemory = (item: MemoryCatalogItem) => {
    if (item.id === memoryId) {
      if (!detailTrace) loadTrace(item.id);
      return;
    }
    navigate(`/memory/explorer/${scope}/${encodeURIComponent(item.id)}`);
  };
  const showList = () => {
    setTrace(null);
    navigate(`/memory/explorer/${scope}`);
  };
  const changeScope = (next: MemoryScope) => {
    setTrace(null);
    setQuery("");
    setType("all");
    setTag("all");
    setImportant("all");
    setMandatory("all");
    navigate(`/memory/explorer/${next}`);
  };

  const save = async (value: MemoryWrite | MemoryUpdate) => {
    setSaving(true);
    try {
      const result =
        form === "edit" && detailTrace?.current
          ? projectId
            ? await memoryApi.update(projectDir, scope, detailTrace.current.id, value as MemoryUpdate, projectId)
            : await memoryApi.update(projectDir, scope, detailTrace.current.id, value as MemoryUpdate)
          : projectId
            ? await memoryApi.create(projectDir, scope, value as MemoryWrite, projectId)
            : await memoryApi.create(projectDir, scope, value as MemoryWrite);
      setTrace(result);
      setForm(null);
      navigate(
        `/memory/explorer/${scope}/${encodeURIComponent(result.memory_id)}`,
        { replace: true },
      );
      await loadCatalog();
      showToast(
        form === "edit" ? "Memory revision saved" : "Memory created",
        "success",
      );
    } catch {
      showToast("Failed to save memory", "error");
    } finally {
      setSaving(false);
    }
  };

  const remove = async () => {
    if (!detailTrace?.current) return;
    if (
      !window.confirm(
        `Remove “${detailTrace.current.title}”? The current record will be removed and its final revision retained for traceability.`,
      )
    )
      return;
    try {
      if (projectId) await memoryApi.remove(projectDir, scope, detailTrace.current.id, projectId);
      else await memoryApi.remove(projectDir, scope, detailTrace.current.id);
      setTrace(null);
      navigate(`/memory/explorer/${scope}`, { replace: true });
      await loadCatalog();
      showToast("Memory removed", "success");
    } catch {
      showToast("Failed to remove memory", "error");
    }
  };

  usePageRefresh(() => refreshAll([loadCatalog(), memoryId ? loadTrace(memoryId) : Promise.resolve()]));
  return (
    <WorkPage>
      <WorkHeader
        title={scope === "user" ? "Personal memory" : "Project memory"}
        description="Keep decisions, conventions and lessons available beyond a single conversation."
        actions={
          <>

            <button
              className="work-button primary"
              title="Create memory"
              onClick={() => setForm("create")}
            >
              Create memory
            </button>
          </>
        }
      />
      <WorkTabs
        value={scope}
        onChange={(id) => changeScope(id as MemoryScope)}
        items={[
          ["project", "Project"],
          ["user", "User"],
        ]}
        label="Memory scope"
      />
      <div className="work-toolbar">
        <WorkSearch label="Search memories" value={query} onChange={setQuery} />
        <StyledSelect
          aria-label="Filter memory type"
          value={type}
          onChange={(e) => setType(e.target.value)}
        >
          <option value="all">All types</option>
          {memoryTypes.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </StyledSelect>
        <StyledSelect
          aria-label="Filter memory tag"
          value={tag}
          onChange={(e) => setTag(e.target.value)}
        >
          <option value="all">All tags</option>
          {catalog.tags.map((t) => (
            <option key={t}>{t}</option>
          ))}
        </StyledSelect>
        <StyledSelect
          aria-label="Filter importance"
          value={important}
          onChange={(e) => setImportant(e.target.value)}
        >
          <option value="all">Any importance</option>
          <option value="true">Important</option>
          <option value="false">Not important</option>
        </StyledSelect>
        <StyledSelect
          aria-label="Filter mandatory"
          value={mandatory}
          onChange={(e) => setMandatory(e.target.value)}
        >
          <option value="all">Any startup policy</option>
          <option value="true">Mandatory</option>
          <option value="false">Not mandatory</option>
        </StyledSelect>
      </div>
      <section aria-label="Memory catalogue">
        <div className="work-catalogue-summary">
          <span>{catalog.total} current memories</span>
          <span>History stays attached to each record.</span>
        </div>
        {loading ? (
          <LoadingSpinner label="Loading memories…" />
        ) : catalog.results.length ? (
          <div className="work-table-wrap work-catalogue">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Guidance</th>
                  <th>Type</th>
                  <th>Policy</th>
                  <th>Updated</th>
                </tr>
              </thead>
              <tbody>
                {catalog.results.map((item) => (
                  <tr
                    key={item.id}
                    className={memoryId === item.id ? "selected" : ""}
                  >
                    <td>
                      <button
                        className="record-title"
                        onClick={() => selectMemory(item)}
                      >
                        {item.title}
                      </button>
                      <div className="markdown-preview"><MemoryMarkdown content={item.snippet || ""} /></div>
                    </td>
                    <td>
                      <TypeBadge type={item.type} />
                    </td>
                    <td>
                      <MemoryFlags
                        important={item.important}
                        mandatory={item.mandatory}
                      />
                    </td>
                    <td>
                      {shortDate(item.updated_at)}
                      <small>Revision {item.revision}</small>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <WorkEmpty title="No memories match this view">
            Adjust filters or capture a new durable memory.
          </WorkEmpty>
        )}
      </section>
      {detailTrace ? (
        <MemoryDetail
          key={detailTrace.memory_id}
          trace={detailTrace}
          onBack={showList}
          onEdit={() => setForm("edit")}
          onRemove={() => void remove()}
        />
      ) : !loading && catalog.results.length > 0 ? (
        <WorkEmpty title="Select a memory">
          Choose a current record to inspect its guidance, scope and complete
          revision trace.
        </WorkEmpty>
      ) : null}
      {form && (
        <MemoryForm
          key={form + "-" + (detailTrace?.current?.key || "new")}
          initial={form === "edit" ? detailTrace?.current : undefined}
          saving={saving}
          onClose={() => setForm(null)}
          onSave={(value) => void save(value)}
        />
      )}
    </WorkPage>
  );
}
