import { StyledSelect } from "@/components/shared/StyledSelect";
import { RecordReferences } from "@/components/shared/RecordReferences";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
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
  ArrowLeft,
  Check,
  CheckCircle2,
  ChevronDown,
  Clock3,
  GitBranch,
  RefreshCw,
  Search,
  Users,
} from "lucide-react";

import {
  sessionApi,
  type SessionDetail,
  type SessionSearchResult,
  type SessionSpec,
} from "@/api/taskSession";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { MarkdownContent } from "@/components/wiki/WikiMarkdown";
import { showToast } from "@/hooks/useToast";
import { cn } from "@/lib/utils";
import { useAppStore } from "@/store/appStore";

import { TaskModeToggle } from "./TaskModeToggle";

const statusStyle: Record<string, string> = {
  open: "bg-blue-500/10 text-blue-600 dark:text-blue-300 border-blue-500/20",
  in_progress:
    "bg-amber-500/10 text-amber-700 dark:text-amber-300 border-amber-500/20",
  completed:
    "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border-emerald-500/20",
  cancelled:
    "bg-rose-500/10 text-rose-700 dark:text-rose-300 border-rose-500/20",
};

const statusLabel = (status: string) => status.replace("_", " ");

function StatusBadge({ status }: { status: string }) {
  return (
    <span
      className={cn(
        "status-pill",
        statusStyle[status] ?? "bg-accent text-muted-foreground border-border",
      )}
    >
      {statusLabel(status)}
    </span>
  );
}

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
] as const;

function StatusSelector({value,onChange}:{value:string;onChange:(value:string)=>void}) {
 return <StyledSelect aria-label="Filter session status" menuLabel="Session statuses" className="flex-1" value={value} onChange={e=>onChange(e.target.value)}>
 {statusOptions.map(([id,label])=><option key={id} value={id}>{label}</option>)}
 </StyledSelect>;
}

function SessionMarkdown({ content }: { content: string }) {
  return (
    <div className="min-w-0 [&_.wiki-prose>*:last-child]:mb-0">
      <MarkdownContent content={content} />
    </div>
  );
}

function SpecSnapshot({ label, spec }: { label: string; spec: SessionSpec }) {
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
          <SessionMarkdown content={spec.description} />
        </div>
      ) : (
        <p className="mt-2 text-sm text-muted-foreground">No description.</p>
      )}
      {spec.strategy && (
        <div className="mt-3 border-t border-border/30 pt-3">
          <p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
            Strategy
          </p>
          <SessionMarkdown content={spec.strategy} />
        </div>
      )}
    </div>
  );
}

function SessionDetailView({
  detail,
  onBack,
  onOpenTask,
}: {
  detail: SessionDetail;
  onBack: () => void;
  onOpenTask: (id: string) => void;
}) {
  const { session, checkpoints, spec_revisions: revisions } = detail;
  return (
    <article className="work-dossier" aria-label="Session brief">
      <header className="dossier-header">
        <div>
          <div className="dossier-status">
            <StatusBadge status={session.status} />
            <small>
              <code>{session.id}</code> / revision {session.revision}
            </small>
          </div>
          <h2>{session.title}</h2>
        </div>
        <button className="work-button" onClick={onBack}>
          Sessions
        </button>
      </header>
      <nav className="dossier-nav" aria-label="Session record sections">
        <a href="#session-intent">Request & strategy</a>
        <a href="#session-work">Linked tasks</a>
        <a href="#session-checkpoints">Checkpoints</a>
        <a href="#session-revisions">Revisions</a>
        <a href="#session-lifecycle">Lifecycle</a>
      </nav>
      <div className="dossier-layout">
        <div className="dossier-content">
          {session.next_step && (
            <WorkNotice title="Next step">
              <SessionMarkdown content={session.next_step} />
            </WorkNotice>
          )}
          {session.progress_summary && (
            <WorkSection title="Latest progress">
              <SessionMarkdown content={session.progress_summary} />
            </WorkSection>
          )}
          <WorkSection id="session-intent" title="Request">
            <SessionMarkdown content={session.description} />
          </WorkSection>
          <WorkSection title="Strategy">
            <SessionMarkdown content={session.strategy} />
          </WorkSection>
          <RecordReferences kind="session" id={session.id} />
          <WorkSection
            id="session-work"
            title="Linked tasks"
            description="Follow each delivery into its requirements and acceptance evidence."
          >
            {detail.tasks.map((task) => (
              <RecordLink
                key={task.id}
                title={task.title}
                meta={task.id}
                onClick={() => onOpenTask(task.id)}
              >
                <StatusBadge status={task.status} />
              </RecordLink>
            ))}
            {!detail.tasks.length && (
              <p>No tasks are linked to this session yet.</p>
            )}
          </WorkSection>
          <WorkSection
            id="session-checkpoints"
            title="Checkpoints"
            description="The decisions and handoffs that keep this request moving."
          >
            <div className="work-timeline">
              {checkpoints.map((c) => (
                <article key={c.key}>
                  <time>
                    {shortDate(c.at)} / rev {c.revision}
                  </time>
                  <SessionMarkdown content={c.summary} />
                  {c.problems && (
                    <WorkNotice title="Problems" tone="error">
                      <SessionMarkdown content={c.problems} />
                    </WorkNotice>
                  )}
                  {c.decisions && (
                    <div>
                      <h3>Decisions</h3>
                      <SessionMarkdown content={c.decisions} />
                    </div>
                  )}
                  {c.strategy && (
                    <div>
                      <h3>Strategy</h3>
                      <SessionMarkdown content={c.strategy} />
                    </div>
                  )}
                  {c.next_step && (
                    <div>
                      <h3>Next step</h3>
                      <SessionMarkdown content={c.next_step} />
                    </div>
                  )}
                  <small>{c.actor}</small>
                </article>
              ))}
            </div>
            {!checkpoints.length && <p>No checkpoints recorded.</p>}
          </WorkSection>
          <WorkSection id="session-revisions" title="Specification revisions">
            {revisions.map((r) => (
              <details className="work-disclosure" key={r.key}>
                <summary>rev {r.source_revision}</summary>
                <div>
                  <SessionMarkdown content={r.reason} />
                  <small>
                    {r.actor} · {shortDate(r.at)}
                  </small>
                  <div className="work-two-columns">
                    <SpecSnapshot label="Before" spec={r.before} />
                    <SpecSnapshot label="After" spec={r.after} />
                  </div>
                </div>
              </details>
            ))}
            {!revisions.length && <p>No specification revisions recorded.</p>}
          </WorkSection>
          <WorkSection
            id="session-lifecycle"
            title="Lifecycle"
            description="Ownership and state transitions in the authoritative session record."
          >
            <div className="work-timeline">
              {detail.events.map((event) => (
                <article key={event.key}>
                  <time>
                    {shortDate(event.at)} / rev {event.revision}
                  </time>
                  <h3>{event.type}</h3>
                  {event.from_status && (
                    <p>
                      {event.from_status} → {event.to_status}
                    </p>
                  )}
                  <SessionMarkdown content={event.summary} />
                  {event.next_step && (
                    <WorkNotice title="Next step">
                      <SessionMarkdown content={event.next_step} />
                    </WorkNotice>
                  )}
                  <small>{event.actor}</small>
                </article>
              ))}
            </div>
            {!detail.events.length && <p>No lifecycle events recorded.</p>}
          </WorkSection>
          <details className="work-disclosure">
            <summary>Complete JSON record</summary>
            <pre className="work-code">{JSON.stringify(detail, null, 2)}</pre>
          </details>
        </div>
        <aside className="dossier-context">
          <h3>Continuity</h3>
          <FactList
            items={[
              ["Owner", session.owner || "Unclaimed"],
              ["Created", shortDate(session.created_at)],
              ["Updated", shortDate(session.updated_at)],
              ["Lease", shortDate(session.lease_expires_at)],
              ["Claim epoch", session.claim_epoch],
              ["Checkpoints", session.checkpoint_sequence],
              ["Completed by", session.completed_by],
              ["Completed at", shortDate(session.completed_at)],
            ]}
          />
          <WorkNotice title="One evolving request">
            The session preserves intent across tasks and agent handoffs.
          </WorkNotice>
        </aside>
      </div>
    </article>
  );
}

export default function SessionExplorerPage() {
  const { sessionId } = useParams<{ sessionId: string }>();
  const navigate = useNavigate();
  const { activeProjectDir, projectName } = useAppStore();
  const [catalog, setCatalog] = useState<SessionSearchResult[]>([]);
  const [nextCursor, setNextCursor] = useState("");
  const [detailResult, setDetailResult] = useState<{
    projectDir: string;
    selectedID: string;
    detail: SessionDetail;
  } | null>(null);
  const [selectedID, setSelectedID] = useState(
    sessionId ? decodeURIComponent(sessionId) : "",
  );
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState("all");
  const [activeOnly, setActiveOnly] = useState(false);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const selectedIDRef = useRef(selectedID);
  const catalogRequestRef = useRef(0);
  const detailRequestRef = useRef(0);
  const previousProjectRef = useRef(activeProjectDir);
  const detail =
    detailResult?.projectDir === activeProjectDir &&
    detailResult.selectedID === selectedID
      ? detailResult.detail
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
        const page = await sessionApi.list({
          projectDir: activeProjectDir || undefined,
          query: query.trim() || undefined,
          status,
          active: activeOnly,
          pageSize: 20,
          cursor: cursor || undefined,
        });
        if (request !== catalogRequestRef.current) return;
        setCatalog((current) =>
          append
            ? [
                ...new Map(
                  [...current, ...page.results].map((session) => [
                    session.id,
                    session,
                  ]),
                ).values(),
              ]
            : page.results,
        );
        setNextCursor(page.next_cursor);
        if (!append && !selectedIDRef.current && page.results[0]) {
          setSelectedID(page.results[0].id);
          selectedIDRef.current = page.results[0].id;
        }
        window.document.title = `Graphit Sessions — ${projectName || "Explorer"}`;
      } catch {
        if (request !== catalogRequestRef.current) return;
        showToast("Failed to load sessions", "error");
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
    [activeProjectDir, projectName, query, status, activeOnly],
  );

  useEffect(() => {
    const timer = window.setTimeout(() => {
      void loadCatalog();
    }, 200);
    return () => window.clearTimeout(timer);
  }, [loadCatalog]);

  useEffect(() => {
    if (previousProjectRef.current === activeProjectDir) return;
    previousProjectRef.current = activeProjectDir;
    selectedIDRef.current = "";
    setSelectedID("");
    setDetailResult(null);
    navigate("/task/sessions", { replace: true });
  }, [activeProjectDir, navigate]);

  const loadDetail = useCallback(
    (id: string) => {
      const request = ++detailRequestRef.current;
      return sessionApi
        .get(activeProjectDir || undefined, id)
        .then((detail) => {
          if (request === detailRequestRef.current) {
            setDetailResult({
              projectDir: activeProjectDir,
              selectedID: id,
              detail,
            });
          }
        })
        .catch(() => {
          if (request === detailRequestRef.current)
            showToast("Failed to load session details", "error");
        });
    },
    [activeProjectDir],
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
  const selectSession = (id: string) => {
    if (id === selectedID) {
      if (!detail) loadDetail(id);
    } else {
      setSelectedID(id);
      selectedIDRef.current = id;
    }
    navigate(`/task/sessions/${encodeURIComponent(id)}`, { replace: true });
  };
  const showSessionList = () => {
    setSelectedID("");
    selectedIDRef.current = "";
    setDetailResult(null);
    navigate("/task/sessions", { replace: true });
  };
  const openTask = (id: string) =>
    navigate(`/task/explorer/${encodeURIComponent(id)}`);

  usePageRefresh(() => refreshAll([loadCatalog(), selectedID ? loadDetail(selectedID) : Promise.resolve()]));
  return (
    <WorkPage>
      <WorkHeader
        title="Sessions"
        description="Resume the request with its decisions, ownership and next action intact."
      />
      <TaskModeToggle mode="sessions" />
      <div className="work-toolbar">
        <WorkSearch
          label="Search sessions"
          value={query}
          onChange={setQuery}
          placeholder="Find a request, strategy or decision"
        />
        <StatusSelector value={status} onChange={setStatus} />
        <button
          className="work-button"
          aria-pressed={activeOnly}
          title="Show only unfinished sessions"
          onClick={() => setActiveOnly((x) => !x)}
        >
          Active only
        </button>
      </div>
      <div className="work-catalogue-summary">
        <span>
          {catalog.length}
          {nextCursor ? "+" : ""} sessions in this view
        </span>
        <span>Open a continuity brief</span>
      </div>
      <div
        className="work-table-wrap work-catalogue"
        aria-label="Session catalogue"
      >
        <table className="work-table">
          <thead>
            <tr>
              <th>Request</th>
              <th>Status</th>
              <th>Coordinator</th>
              <th>Updated</th>
            </tr>
          </thead>
          <tbody>
            {catalog.map((session) => (
              <tr
                key={session.id}
                className={selectedID === session.id ? "selected" : ""}
              >
                <td>
                  <button
                    className="record-title"
                    onClick={() => selectSession(session.id)}
                  >
                    {session.title}
                  </button>
                  <small>
                    <code>{session.id}</code>
                  </small>
                </td>
                <td>
                  <StatusBadge status={session.status} />
                </td>
                <td>{session.owner || "Unclaimed"}</td>
                <td>{shortDate(session.updated_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <LoadingSpinner size="sm" />}
        {!loading && !catalog.length && (
          <WorkEmpty title="No sessions match this view.">
            Try another filter or project.
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
        <SessionDetailView
          detail={detail}
          onBack={showSessionList}
          onOpenTask={openTask}
        />
      ) : (
        <WorkEmpty title="Select a session">
          Choose a session to inspect its evolving request and linked tasks.
        </WorkEmpty>
      )}
    </WorkPage>
  );
}
