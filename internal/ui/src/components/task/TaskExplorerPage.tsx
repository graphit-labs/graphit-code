import { StyledSelect } from "@/components/shared/StyledSelect";
import { RecordReferences } from "@/components/shared/RecordReferences";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
  WorkBadge,
  WorkStatusBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  FactList,
  WorkNotice,
  WorkSearch,
  WorkEmpty,
  RecordLink,
} from "@/components/shared/EngineeringUI";
import { useCallback, useEffect, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  AlertTriangle,
  ArrowLeft,
  Check,
  CheckCircle2,
  ChevronDown,
  Circle,
  Clock3,
  Download,
  Flag,
  GitBranch,
  MessageSquareText,
  RefreshCw,
  Search,
  ShieldCheck,
  Workflow,
  XCircle,
} from "lucide-react";

import {
  taskApi,
  type TaskCatalogItem,
  type TaskExportDocument,
  type TaskSpec,
} from "@/api/task";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { MarkdownContent } from "@/components/wiki/WikiMarkdown";
import { showToast } from "@/hooks/useToast";
import { projectRequestScope } from "@/lib/projectScope";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";

import { TaskModeToggle } from "./TaskModeToggle";

const statusLabel = (status: string) => status.replace(/_/g, " ");

function shortDate(value?: string) {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

const statusOptions = [
  ["all", "All statuses"],
  ["open", "Open"],
  ["in_progress", "In progress"],
  ["completed", "Completed"],
  ["cancelled", "Cancelled"],
  ["blocked", "Blocked"],
  ["flagged", "Flagged"],
] as const;

function StatusSelector({value,onChange}:{value:string;onChange:(value:string)=>void}) {
 return <StyledSelect aria-label="Filter task status" menuLabel="Task statuses" className="flex-1" value={value} onChange={e=>onChange(e.target.value)}>
 {statusOptions.map(([id,label])=><option key={id} value={id}>{label}</option>)}
 </StyledSelect>;
}

function downloadJSON(document: TaskExportDocument, filename: string) {
  const blob = new Blob([JSON.stringify(document, null, 2) + "\n"], {
    type: "application/json",
  });
  const url = URL.createObjectURL(blob);
  const anchor = window.document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function TaskMarkdown({ content }: { content: string }) {
  return (
    <div className="min-w-0 [&_.wiki-prose>*:last-child]:mb-0">
      <MarkdownContent content={content} />
    </div>
  );
}

function SpecificationSnapshot({
  label,
  spec,
}: {
  label: string;
  spec: TaskSpec;
}) {
  const checks = spec.checks ?? [];
  return (
    <div className="work-panel">
      <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
        {label}
      </p>
      {spec.title && (
        <p className="mt-2 text-sm font-bold text-foreground">{spec.title}</p>
      )}
      {spec.description ? (
        <div className="mt-3">
          <TaskMarkdown content={spec.description} />
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">
          No specification content.
        </p>
      )}
      {checks.length > 0 && (
        <div className="mt-4 space-y-2 border-t border-border/30 pt-3">
          {checks.map((check) => (
            <div key={check.id}>
              <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
                {check.kind} · {check.status}
              </p>
              <TaskMarkdown content={check.text} />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// Rendered as a span with role="button" rather than a real <button>: this chip must be
// usable inside the task list row, which is itself a <button>, and nested buttons are
// invalid HTML that breaks native click/keyboard handling.
function SessionChip({
  sessionId,
  onOpenSession,
}: {
  sessionId: string;
  onOpenSession: (id: string) => void;
}) {
  const open = () => onOpenSession(sessionId);
  return (
    <WorkBadge
      role="button"
      tabIndex={0}
      onClick={(event) => {
        event.stopPropagation();
        open();
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          event.stopPropagation();
          open();
        }
      }}
      title="Open linked session"
      tone="info"
    >
      <Workflow aria-hidden="true" /> Session
    </WorkBadge>
  );
}

function TaskDetail({
  document,
  onBack,
  onOpenSession,
}: {
  document: TaskExportDocument;
  onBack: () => void;
  onOpenSession: (id: string) => void;
}) {
  const task =
    document.tasks.find((item) => item.id === document.task_id) ??
    document.tasks[0];
  if (!task)
    return (
      <WorkEmpty title="Task not found">
        The exported task document is empty.
      </WorkEmpty>
    );
  const subtasks = document.tasks.filter((item) => item.parent_id === task.id);
  const dependencies = document.dependencies.filter(
    (item) => item.task_id === task.id,
  );
  const checks = document.checks.filter((item) => item.task_id === task.id);
  const events = document.events.filter((item) => item.task_id === task.id);
  const comments = document.comments.filter((item) => item.task_id === task.id);
  const revisions = document.spec_revisions.filter(
    (item) => item.task_id === task.id,
  );
  return (
    <article className="work-dossier" aria-label="Task dossier">
      <header className="dossier-header">
        <div>
          <div className="dossier-status">
            <WorkStatusBadge status={task.status} />
            <span>Priority {task.priority}</span>
            {task.flagged && <span>Flagged</span>}
            {task.session_id && (
              <SessionChip
                sessionId={task.session_id}
                onOpenSession={onOpenSession}
              />
            )}
          </div>
          <h2>{task.title}</h2>
          <small>
            <code>{task.id}</code> / revision {task.revision} / {task.type}
          </small>
        </div>
        <div className="work-actions">
          <button className="work-button" onClick={onBack}>
            Tasks
          </button>
          <button
            className="work-button"
            onClick={() => downloadJSON(document, `${task.id}.json`)}
          >
            <Download size={15} />
            Export JSON
          </button>
        </div>
      </header>
      <nav className="dossier-nav" aria-label="Task record sections">
        <a href="#task-evidence">Checks & evidence</a>
        <a href="#task-scope">Specification</a>
        <a href="#task-history">Lifecycle</a>
        <a href="#task-revisions">Revisions</a>
      </nav>
      <div className="dossier-layout">
        <div className="dossier-content">
          {task.flag_reason && (
            <WorkNotice title="Flag reason" tone="error">
              <TaskMarkdown content={task.flag_reason} />
            </WorkNotice>
          )}
          {task.next_step && (
            <WorkNotice title="Next step">
              <TaskMarkdown content={task.next_step} />
            </WorkNotice>
          )}
          {task.progress_summary && (
            <WorkSection title="Latest progress">
              <TaskMarkdown content={task.progress_summary} />
            </WorkSection>
          )}
          <WorkSection
            id="task-evidence"
            title="Checks"
            description="Completion is supported by recorded checks and their evidence."
            actions={
              <WorkBadge>
                {checks.filter((c) => c.active && c.status === "passed").length}{" "}
                / {checks.filter((c) => c.active).length} active checks passed
              </WorkBadge>
            }
          >
            <ul className="evidence-list">
              {checks.map((check) => (
                <li key={check.key}>
                  {check.status === "passed" ? (
                    <CheckCircle2 size={18} />
                  ) : check.status === "failed" ? (
                    <XCircle size={18} />
                  ) : (
                    <Circle size={18} />
                  )}
                  <div>
                    <small>
                      {check.kind} · {check.status}
                      {!check.active ? " · superseded" : ""}
                    </small>
                    <TaskMarkdown content={check.text} />
                    {check.evidence && (
                      <div className="evidence-body">
                        <strong>Evidence</strong>
                        <TaskMarkdown content={check.evidence} />
                        {check.verified_by && (
                          <small>
                            {check.verified_by} / {shortDate(check.verified_at)}
                          </small>
                        )}
                      </div>
                    )}
                    {check.superseded_reason && (
                      <WorkNotice title="Superseded check">
                        <TaskMarkdown content={check.superseded_reason} />
                      </WorkNotice>
                    )}
                  </div>
                </li>
              ))}
            </ul>
            {!checks.length && <p>No checks recorded.</p>}
          </WorkSection>
          <WorkSection id="task-scope" title="Specification">
            <TaskMarkdown content={task.description} />
          </WorkSection>
          <WorkSection title="Comments">
            <div className="work-timeline">
              {comments.map((c) => (
                <article key={c.id}>
                  <small>
                    {c.kind} · {c.actor} · {shortDate(c.at)}
                  </small>
                  <TaskMarkdown content={c.body} />
                </article>
              ))}
            </div>
            {!comments.length && <p>No comments recorded.</p>}
          </WorkSection>
          <WorkSection id="task-history" title="Lifecycle">
            <div className="work-timeline">
              {events.map((e) => (
                <article key={e.key}>
                  <time>
                    {shortDate(e.at)} / rev {e.revision}
                  </time>
                  <h3>{statusLabel(e.type)}</h3>
                  {e.summary && <TaskMarkdown content={e.summary} />}{" "}
                  {e.next_step && (
                    <div>
                      <strong>Next step</strong>
                      <TaskMarkdown content={e.next_step} />
                    </div>
                  )}
                  <small>{e.actor}</small>
                </article>
              ))}
            </div>
          </WorkSection>
          <WorkSection id="task-revisions" title="Specification revisions">
            {revisions.map((r) => (
              <details className="work-disclosure" key={r.key}>
                <summary>
                  rev {r.source_revision} · {statusLabel(r.kind)}
                </summary>
                <div>
                  <TaskMarkdown content={r.reason} />
                  <small>
                    {r.actor} · {shortDate(r.at)}
                  </small>
                  <div className="work-two-columns">
                    <SpecificationSnapshot label="Before" spec={r.before} />
                    <SpecificationSnapshot label="After" spec={r.after} />
                  </div>
                </div>
              </details>
            ))}
            {!revisions.length && <p>No specification revisions recorded.</p>}
          </WorkSection>
          <details className="work-disclosure">
            <summary>Complete JSON document</summary>
            <pre className="work-code">{JSON.stringify(document, null, 2)}</pre>
          </details>
        </div>
        <aside className="dossier-context">
          <h3>Accountability</h3>
          <FactList
            items={[
              ["Owner", task.owner || "Unclaimed"],
              ["Created", shortDate(task.created_at)],
              ["Updated", shortDate(task.updated_at)],
              ["Lease", shortDate(task.lease_expires_at)],
              ["Claim epoch", task.claim_epoch],
              ["Ready", task.ready ? "Ready for execution" : "Not ready"],
            ]}
          />
          <RecordReferences kind="task" id={task.id} />
          {task.session_id && (
            <button
              className="work-button"
              onClick={() => onOpenSession(task.session_id!)}
            >
              Open session
            </button>
          )}
        </aside>
      </div>
    </article>
  );
}

export default function TaskExplorerPage() {
  const { taskId } = useParams<{ taskId: string }>();
  const navigate = useNavigate();
  const { activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId, projectName } = useAppStore();
  const { key: projectKey, projectDir, projectId } = projectRequestScope({
    activeProjectKey, activeProjectOrigin, activeProjectDir, activeProjectId,
  });
  const [catalog, setCatalog] = useState<TaskCatalogItem[]>([]);
  const [nextCursor, setNextCursor] = useState("");
  const [detailResult, setDetailResult] = useState<{
    projectKey: string;
    selectedID: string;
    document: TaskExportDocument;
  } | null>(null);
  const [selectedID, setSelectedID] = useState(
    taskId ? decodeURIComponent(taskId) : "",
  );
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [exporting, setExporting] = useState(false);
  const selectedIDRef = useRef(selectedID);
  const catalogRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const previousProjectRef = useRef(projectKey);
  const detail =
    detailResult?.projectKey === projectKey &&
    detailResult.selectedID === selectedID
      ? detailResult.document
      : null;

  const loadCatalog = useCallback(
    async (cursor = "", append = false) => {
      const request = ++catalogRequestRef.current;
      if (append) setLoadingMore(true);
      else {
        setLoading(true);
        setCatalog([]);
        setNextCursor("");
      }
      try {
        const page = await taskApi.list({
          projectDir,
          ...(projectId ? { projectId } : {}),
          query: query.trim() || undefined,
          status,
          pageSize: 20,
          cursor: cursor || undefined,
        });
        if (request !== catalogRequestRef.current) return;
        setCatalog((current) =>
          append
            ? [
                ...new Map(
                  [...current, ...page.results].map((task) => [task.id, task]),
                ).values(),
              ]
            : page.results,
        );
        setNextCursor(page.next_cursor);
        if (!append && !selectedIDRef.current && page.results[0]) {
          setSelectedID(page.results[0].id);
          selectedIDRef.current = page.results[0].id;
        }
        window.document.title = `Graphit Tasks — ${projectName || "Explorer"}`;
      } catch {
        if (request !== catalogRequestRef.current) return;
        showToast("Failed to load tasks", "error");
        if (!append) {
          setCatalog([]);
          setNextCursor("");
        }
      } finally {
        if (request === catalogRequestRef.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [projectDir, projectId, projectName, query, status],
  );

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadCatalog();
    }, 200);
    return () => window.clearTimeout(timer);
  }, [loadCatalog]);

  useEffect(() => {
    if (previousProjectRef.current === projectKey) return;
    previousProjectRef.current = projectKey;
    selectedIDRef.current = "";
    setSelectedID("");
    setDetailResult(null);
    navigate("/task/explorer", { replace: true });
  }, [projectKey, navigate]);

  const loadDetail = useCallback(
    (id: string) => {
      const request = ++detailRequestRef.current;
      const requestDetail = projectId
        ? taskApi.export(projectDir, id, projectId)
        : taskApi.export(projectDir, id);
      return requestDetail
        .then((document) => {
          if (request === detailRequestRef.current) {
            setDetailResult({
              projectKey,
              selectedID: id,
              document,
            });
          }
        })
        .catch(() => {
          if (request === detailRequestRef.current)
            showToast("Failed to load task details", "error");
        });
    },
    [projectDir, projectId, projectKey],
  );

  useEffect(() => {
    if (!selectedID) {
      detailRequestRef.current += 1;
      return;
    }
    loadDetail(selectedID);
  }, [loadDetail, selectedID]);

  // Clicking the already selected row must keep its detail on screen. Selecting the same id
  // does not change state, so the effect above cannot run: only reload when nothing is
  // rendered, which recovers from a detail request that failed earlier.
  const selectTask = (id: string) => {
    if (id === selectedID) {
      if (!detail) loadDetail(id);
    } else {
      setSelectedID(id);
      selectedIDRef.current = id;
    }
    navigate(`/task/explorer/${encodeURIComponent(id)}`, { replace: true });
  };
  const showTaskList = () => {
    setSelectedID("");
    selectedIDRef.current = "";
    setDetailResult(null);
    navigate("/task/explorer", { replace: true });
  };
  const openSession = (id: string) =>
    navigate(`/task/sessions/${encodeURIComponent(id)}`);
  const exportAll = async () => {
    setExporting(true);
    try {
      const document = projectId
        ? await taskApi.export(projectDir, undefined, projectId)
        : await taskApi.export(projectDir);
      downloadJSON(document, "graphit-tasks.json");
    } catch {
      showToast("Failed to export tasks", "error");
    } finally {
      setExporting(false);
    }
  };

  usePageRefresh(() => refreshAll([loadCatalog(), selectedID ? loadDetail(selectedID) : Promise.resolve()]));
  return (
    <WorkPage>
      <WorkHeader
        title="Tasks & evidence"
        description="Inspect the work contract, understand what is blocking delivery, and verify the evidence."
      />
      <TaskModeToggle mode="tasks" />
      <div className="work-toolbar">
        <WorkSearch
          label="Search tasks"
          value={query}
          onChange={setQuery}
          placeholder="Find a task by intent, title or ID"
        />
        <StatusSelector value={status} onChange={setStatus} />
        <button
          className="work-button"
          aria-label="Export all tasks"
          disabled={exporting}
          onClick={() => void exportAll()}
        >
          <Download size={15} />
          Export all tasks
        </button>
      </div>
      <div className="work-catalogue-summary">
        <span>
          {catalog.length}
          {nextCursor ? "+" : ""} tasks in this view
        </span>
        <span>Select a record to review its delivery contract</span>
      </div>
      <div
        className="work-table-wrap work-catalogue"
        aria-label="Task catalogue"
      >
        <table className="work-table">
          <thead>
            <tr>
              <th>Work item</th>
              <th>Status</th>
              <th>Owner</th>
              <th>Priority</th>
              <th>Readiness</th>
            </tr>
          </thead>
          <tbody>
            {catalog.map((task) => (
              <tr
                key={task.id}
                className={selectedID === task.id ? "selected" : ""}
              >
                <td>
                  <button
                    className="record-title"
                    onClick={() => selectTask(task.id)}
                  >
                    {task.title}
                  </button>
                  <small>
                    <code>{task.id}</code>{" "}
                    {task.session_id && (
                      <SessionChip
                        sessionId={task.session_id}
                        onOpenSession={openSession}
                      />
                    )}
                  </small>
                </td>
                <td>
                  <WorkStatusBadge status={task.status} />
                </td>
                <td>{task.owner || "Unclaimed"}</td>
                <td>P{task.priority}</td>
                <td>
                  {task.flagged
                    ? "Flagged"
                    : task.blocked_by?.length
                      ? "Blocked"
                      : task.ready
                        ? "Ready"
                        : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <LoadingSpinner size="sm" />}
        {!loading && !catalog.length && (
          <WorkEmpty title="No tasks match this view.">
            Change a filter or choose a project with recorded work.
          </WorkEmpty>
        )}
        {nextCursor && (
          <button
            className="work-button"
            disabled={loadingMore}
            onClick={() => void loadCatalog(nextCursor, true)}
          >
            {loadingMore ? "Loading…" : "Load more"}
          </button>
        )}
      </div>
      {detail ? (
        <TaskDetail
          document={detail}
          onBack={showTaskList}
          onOpenSession={openSession}
        />
      ) : (
        <WorkEmpty title="Select a task">
          Choose a task to inspect its complete deterministic record.
        </WorkEmpty>
      )}
    </WorkPage>
  );
}
