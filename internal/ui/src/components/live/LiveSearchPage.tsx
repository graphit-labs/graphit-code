import { StyledSelect } from "@/components/shared/StyledSelect";
import { usePageRefresh, refreshAll } from "@/components/layout/WorkspaceRefresh";
import {
  WorkStatusBadge,
  WorkPage,
  WorkHeader,
  WorkSection,
  WorkSearch,
  WorkTabs,
  WorkEmpty,
  WorkNotice,
  FactList,
} from "@/components/shared/EngineeringUI";
import { LiveAnswer } from "./LiveAnswer";
import { LiveEvidence } from "./LiveEvidence";
import { ExecutionActivity } from "@/components/shared/ExecutionActivity";
import { AgentExecution, executionEventLabel } from "@/components/shared/AgentExecution";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  AlertCircle,
  Bot,
  Check,
  Clock,
  Layers,
  MessageSquare,
  Monitor,
  PanelLeftClose,
  PanelLeftOpen,
  Plus,
  Radio,
  Search,
  Send,
  Sparkles,
  Square,
  Trash2,
  User,
  Wrench,
} from "lucide-react";
import {
  cancelLiveTurn,
  createLiveSession,
  listLiveSessions,
  removeLiveSession,
  sendLiveMessage,
  subscribeLiveEvents,
  type LiveArtifact,
  type LiveEvent,
  type LiveSession,
  type LiveSubscription,
} from "@/api/live";
import { loadLiveCatalog, filterLiveCatalog, artifactKey, type LiveCatalogEntry } from "@/api/liveCatalog";
import { EmptyState } from "@/components/shared/EmptyState";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useAppStore } from "@/store/appStore";
import { cn } from "@/lib/utils";

interface Turn {
  question?: string;
  answer: string;
  activity: LiveEvent[];
  currentEvent?: LiveEvent;
  errors: string[];
  done: boolean;
}

export function transcriptFromEvents(events: LiveEvent[]): Turn[] {
  const turns: Turn[] = [];
  const current = (): Turn => {
    if (turns.length === 0 || turns[turns.length - 1].done) {
      turns.push({ answer: "", activity: [], errors: [], done: false });
    }
    return turns[turns.length - 1];
  };

  for (const ev of events) {
    switch (ev.kind) {
      case "prompt": {
        const t = current();
        if (t.question !== undefined) {
          turns.push({
            question: ev.text,
            answer: "",
            activity: [],
            errors: [],
            done: false,
          });
        } else {
          t.question = ev.text;
        }
        break;
      }
      case "text":
        current().answer += ev.text ?? "";
        if (ev.text?.trim()) current().currentEvent = ev;
        break;
      case "prep":
      case "thinking":
      case "stdout":
      case "stderr":
      case "tool_result":
      case "tool_use":
        current().activity.push(ev);
        if (executionEventLabel(ev)) current().currentEvent = ev;
        break;
      case "error":
        current().errors.push(ev.text ?? "");
        break;
      case "turn_done":
        current().done = true;
        break;
      default:
        break;
    }
  }
  return turns;
}

type LiveState = LiveSession["state"];

export default function LiveSearchPage() {
  const { activeProjectDir, activeAgent, projectsLoaded, loadProjects } =
    useAppStore();

  const [entries, setEntries] = useState<LiveCatalogEntry[]>([]);
  const [catalogErrors, setCatalogErrors] = useState<string[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [sourceFilter, setSourceFilter] = useState("");
  const [chosen, setChosen] = useState<LiveArtifact[]>([]);
  const [typeFilter, setTypeFilter] = useState("");
  const [pickerQuery, setPickerQuery] = useState("");

  const [question, setQuestion] = useState("");
  const [session, setSession] = useState<LiveSession | null>(null);
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [sessions, setSessions] = useState<LiveSession[]>([]);
  const [starting, setStarting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [sessionsProblem, setSessionsProblem] = useState<string | null>(null);
  const [streamQuiet, setStreamQuiet] = useState(false);
  const [workspace, setWorkspace] = useState("prepare");
  const [evidence, setEvidence] = useState("output");

  const subscription = useRef<LiveSubscription | null>(null);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);

  const refreshSessions = useCallback(() => {
    return listLiveSessions()
      .then(rows => { setSessions(rows); setSessionsProblem(null); })
      .catch((error) => { setSessionsProblem(`Could not refresh recent sessions: ${error instanceof Error ? error.message : String(error)}`); });
  }, []);

  const registryRequest = useRef(0);
  const activeScope = `${activeProjectDir}\0${activeAgent}`;
  const activeScopeRef = useRef(activeScope); useLayoutEffect(() => { activeScopeRef.current = activeScope; }, [activeScope]);
  const loadArtifacts = useCallback(async () => {
    const request = ++registryRequest.current;
    if (!activeAgent) return;
    setCatalogLoading(true);
    try {
      const result = await loadLiveCatalog(activeProjectDir, activeAgent);
      if (request === registryRequest.current) {
        setEntries(result.entries); setCatalogErrors(result.errors);
        const available=new Set(result.entries.filter(e=>!e.unavailable).map(e=>artifactKey(e.ref)));
        setChosen(items=>items.filter(item=>available.has(artifactKey(item))));
      }
    } catch (error) {
      if (request === registryRequest.current) setCatalogErrors([(error as Error).message]);
    } finally { if (request === registryRequest.current) setCatalogLoading(false); }
  }, [activeProjectDir, activeAgent]);
  const [catalogScope, setCatalogScope] = useState(activeScope);
  if (catalogScope !== activeScope) {
    setCatalogScope(activeScope); setChosen([]); setEntries([]); setCatalogErrors([]);
  }
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) void loadArtifacts(); });
    if (!projectsLoaded) void loadProjects();
    void refreshSessions();
    return () => { active = false; registryRequest.current++; };
  }, [loadArtifacts, projectsLoaded, loadProjects, refreshSessions]);
  usePageRefresh(() => refreshAll([loadArtifacts(), refreshSessions()]));

  useEffect(() => {
    subscription.current?.close();
    subscription.current = null;
    if (!session) return;

    const sub = subscribeLiveEvents(session.id, 0, {
      onEvent: (ev) => {
        setEvents((prev) =>
          prev.some((p) => p.seq === ev.seq) ? prev : [...prev, ev],
        );
        if (ev.kind === "state" && ev.state) {
          setSession((s) => (s ? { ...s, state: ev.state as LiveState } : s));
          if (ev.state === "ready" || ev.state === "failed") refreshSessions();
        }
      },
      onOpen: () => setStreamQuiet(false),
      onError: () => setStreamQuiet(true),
    });
    subscription.current = sub;
    return () => {
      sub.close();
    };
  }, [session?.id, refreshSessions]); // eslint-disable-line react-hooks/exhaustive-deps

  const turns = useMemo(() => transcriptFromEvents(events), [events]);

  useEffect(() => {
    if (workspace === "run" && evidence === "output") transcriptEnd.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns]);

  const types = useMemo(() => {
    const seen = new Set<string>();
    for (const e of entries) if (e.type) seen.add(e.type);
    return Array.from(seen).sort();
  }, [entries]);

  const visibleEntries = useMemo(() => {
    return filterLiveCatalog(entries,pickerQuery,typeFilter,sourceFilter);
  }, [entries, typeFilter, pickerQuery,sourceFilter]);

  const isChosen = (e: LiveCatalogEntry) =>
    chosen.some((c) => artifactKey(c) === artifactKey(e.ref));

  const toggle = (e: LiveCatalogEntry) => {
    if(e.unavailable) return;
    setChosen((prev) =>
      prev.some((c) => artifactKey(c) === artifactKey(e.ref))
        ? prev.filter((c) => artifactKey(c) !== artifactKey(e.ref))
        : [...prev, e.ref],
    );
  };

  const start = async () => {
    if (starting || chosen.length === 0 || !activeAgent) return;
    setProblem(null);
    setStarting(true);
    const startScope=activeScopeRef.current;
    try {
      const created = await createLiveSession({
        agent: activeAgent,
        artifacts: chosen,
        prompt: question.trim() || undefined,
      });
      if(startScope!==activeScopeRef.current) {void refreshSessions();return;}
      setEvents([]);
      setSession(created);
      setWorkspace("run");
      setQuestion("");
      refreshSessions();
    } catch (e) {
      if(startScope===activeScopeRef.current) setProblem(e instanceof Error ? e.message : String(e));
    } finally {
      setStarting(false);
    }
  };

  const ask = async () => {
    if (!session || !question.trim()) return;
    setProblem(null);
    const prompt = question;
    setQuestion("");
    try {
      await sendLiveMessage(session.id, prompt);
      setSession((s) => (s ? { ...s, state: "running" } : s));
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
      setQuestion(prompt);
    }
  };

  const stop = async () => {
    if (!session) return;
    try {
      await cancelLiveTurn(session.id);
    } catch (e) {
      setProblem((e as Error).message);
    }
  };

  const remove = async (id: string) => {
    if (
      !window.confirm(
        "Remove this ephemeral live session and its local records?",
      )
    )
      return;
    try {
      await removeLiveSession(id);
      if (session?.id === id) {
        setSession(null);
        setEvents([]);
      }
      refreshSessions();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    }
  };

  const open = (s: LiveSession) => {
    setEvents([]);
    setSession(s);
    setWorkspace("run");
  };

  const busy = session?.state === "preparing" || session?.state === "running";
  const canAsk = session?.state === "ready" && question.trim().length > 0;

  const promptEditor = (
    <div className="work-form">
      <label className="work-field">
        <span>
          {session
            ? "Follow-up question"
            : "What should the agent investigate?"}
        </span>
        <textarea
          value={question}
          onChange={(e) => setQuestion(e.target.value)}
          rows={4}
          placeholder={
            session
              ? "Ask a follow-up query…"
              : "Describe the question and the evidence you need."
          }
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
              e.preventDefault();
              if (session) {
                if (canAsk) void ask();
              } else if (!starting && chosen.length && activeAgent)
                void start();
            }
          }}
        />
      </label>
      <div className="work-actions">
        {session ? (
          <button
            className="work-button primary"
            disabled={!canAsk}
            onClick={() => void ask()}
          >
            Ask
          </button>
        ) : (
          <button
            className="work-button primary"
            disabled={starting || chosen.length === 0 || !activeAgent}
            onClick={() => void start()}
          >
            {starting ? "Preparing…" : "Start Run"}
          </button>
        )}
        <small className="text-xs text-muted-foreground">
          Ctrl / ⌘ + Enter
        </small>
      </div>
    </div>
  );
  return (
    <WorkPage>
      <WorkHeader
        title="Live investigation"
        description="Assemble a bounded set of artifacts and investigate it with an agent in an ephemeral workspace."
        actions={
          <button
            className="work-button"
            onClick={() => {
              setSession(null);
              setEvents([]);
              setWorkspace("prepare");
              setProblem(null);
            }}
          >
            New Search
          </button>
        }
      />
      <WorkTabs
        value={workspace}
        onChange={setWorkspace}
        items={[
          ["prepare", "Prepare context"],
          ["run", "Run & evidence"],
          ["history", "Recent sessions"],
        ]}
        label="Live investigation workspace"
      />
      {sessionsProblem && <WorkNotice title="Recent sessions could not refresh" tone="error">{sessionsProblem}</WorkNotice>}
      {problem && (
        <WorkNotice title="The request could not complete" tone="error">
          {problem}
        </WorkNotice>
      )}
      {workspace === "prepare" && (
        <div className="live-preparation">
          <section>
            <WorkSection
              title="Choose the evidence boundary"
              description="Select the artifacts the agent will use for this investigation."
            >
              <div className="work-toolbar">
                <WorkSearch
                  label="Find live artifacts"
                  value={pickerQuery}
                  onChange={setPickerQuery}
                />
                <StyledSelect aria-label="Artifact source" value={sourceFilter} onChange={e=>setSourceFilter(e.target.value)}>
                  <option value="">All sources</option><option value="project">Current project</option><option value="hub">Hub registry</option>
                </StyledSelect>
                <StyledSelect
                  aria-label="Live artifact type"
                  value={typeFilter}
                  onChange={(e) => setTypeFilter(e.target.value)}
                >
                  <option value="">All types</option>
                  {types.map((t) => (
                    <option key={t}>{t}</option>
                  ))}
                </StyledSelect>
              </div>
              {catalogLoading && <p role="status">Loading project and Hub artifacts…</p>}
              {catalogErrors.map(message=><WorkNotice key={message} title="Some sources could not load" tone="error">{message}</WorkNotice>)}
              <div className="work-table-wrap live-artifacts">
                <table className="work-table">
                  <thead>
                    <tr>
                      <th>Use</th>
                      <th>Artifact</th>
                      <th>Type</th>
                      <th>Version</th>
                    </tr>
                  </thead>
                  <tbody>
                    {visibleEntries.map((e) => (
                      <tr
                        key={artifactKey(e.ref)}
                        className={isChosen(e) ? "selected" : ""}
                      >
                        <td>
                          <input
                            type="checkbox"
                            aria-label={"Use " + e.name}
                            checked={isChosen(e)}
                            disabled={!!e.unavailable}
                            onChange={() => toggle(e)}
                          />
                        </td>
                        <td>
                          <strong>{e.name}</strong>
                          <small>{e.description}</small>
                          <small>{e.id}</small>
                          <small>{e.sourceLabel}{e.ref.project_kind && ` · ${e.ref.project_kind.replace(/_/g,' ')}`}</small>
                          {e.unavailable && <small>{e.unavailable}</small>}
                        </td>
                        <td>{e.type}</td>
                        <td>{e.latest}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                {!visibleEntries.length && !catalogLoading && (
                  <WorkEmpty title="No matching artifacts">
                    Adjust the filter or make an artifact available in the
                    project or registry.
                  </WorkEmpty>
                )}
              </div>
            </WorkSection>
          </section>
          <aside className="work-panel">
            <WorkSection title="Investigation brief">
              <FactList
                items={[
                  ["Agent", activeAgent || "Choose an agent"],
                  ["Selected artifacts", chosen.length],
                ]}
              />
              <div className="selected-artifacts">
                {chosen.map((c) => (
                  <div key={artifactKey(c)}>
                    <span>
                      <strong>{c.id}</strong>
                      <small>
                        {c.type} · {c.version} · {entries.find(e=>artifactKey(e.ref)===artifactKey(c))?.sourceLabel || c.source || 'Hub'}
                      </small>
                    </span>
                    <button
                      className="work-button"
                      aria-label={"Remove " + c.id}
                      onClick={() =>
                        setChosen((prev) =>
                          prev.filter(
                            (a) => artifactKey(a) !== artifactKey(c),
                          ),
                        )
                      }
                    >
                      Remove
                    </button>
                  </div>
                ))}
              </div>
              {session ? (
                <WorkNotice title="A session is already open">
                  Context changes apply to the next run.
                  <button
                    className="work-button mt-3"
                    onClick={() => {
                      setSession(null);
                      setEvents([]);
                      setQuestion("");
                    }}
                  >
                    Prepare a new run
                  </button>
                </WorkNotice>
              ) : (
                promptEditor
              )}
            </WorkSection>
            <p className="text-xs text-muted-foreground">
              Live sessions use an ephemeral project. They are separate from
              durable task sessions.
            </p>
          </aside>
        </div>
      )}
      {workspace === "run" &&
        (session ? (
          <>
            <div className="runtime-strip">
              <strong>{session.title || "Live session"}</strong>
              <WorkStatusBadge status={session.state} />
              <code>{session.id}</code>
              {session.state === "running" && (
                <button
                  className="work-button danger"
                  onClick={() => void stop()}
                >
                  Stop Run
                </button>
              )}
            </div>
            {streamQuiet && (
              <WorkNotice title="Reconnecting">
                The connection paused. The agent run continues on the server.
              </WorkNotice>
            )}
            <div className="live-run-layout">
              <section className="work-panel">
                <LiveEvidence key={session.id} sessionId={session.id} value={evidence} onChange={setEvidence}
                  activity={<ExecutionActivity events={events} allowLegacyPairing />}
                  output={onLink => (<div className="live-transcript">
                    {turns.map((t, i) => (
                      <article key={i}>
                        <header>
                          <small>Request {i + 1}</small>
                          <h3>{t.question || "Agent preparation"}</h3>
                        </header>
                        <AgentExecution events={t.activity} currentEvent={t.currentEvent} running={!t.done && busy} outcome={t.errors.includes("cancelled") ? "cancelled" : t.errors.length ? "failed" : t.done ? "completed" : "idle"} />
                        {t.answer && (
                          <div className="agent-answer">
                            <LiveAnswer content={t.answer} onLink={onLink} />
                          </div>
                        )}
                        {t.errors.map((e, j) => (
                          <WorkNotice key={j} title="Run error" tone="error">
                            {e}
                          </WorkNotice>
                        ))}
                        <small className="text-muted-foreground">
                          {t.done
                            ? t.errors.includes("cancelled") ? "Turn stopped" : t.errors.length ? "Turn failed" : "Turn complete"
                            : busy
                              ? ""
                              : "Waiting"}
                        </small>
                      </article>
                    ))}
                    {!turns.length && (
                      <WorkEmpty
                        title={
                          busy
                            ? "Preparing the investigation"
                            : "No output recorded"
                        }
                      >
                        {busy
                          ? "The agent environment is being prepared."
                          : "Session state: " + session.state}
                      </WorkEmpty>
                    )}
                    <div ref={transcriptEnd} />
                  </div>)}
                />
                <div className="live-composer">{promptEditor}</div>
              </section>
              <aside>
                <WorkSection title="Run context">
                  <FactList
                    items={[
                      ["Agent", session.agent],
                      ["Created", session.created_at],
                      ["Updated", session.updated_at],
                      ["State", session.state],
                    ]}
                  />
                </WorkSection>
                <WorkSection title="Artifacts used">
                  {(session.artifacts || []).map((a) => (
                    <div
                      className="py-3 border-b border-border text-xs"
                      key={a.type + "/" + a.id}
                    >
                      <strong className="break-all">{a.id}</strong>
                      <small className="block mt-2 text-muted-foreground">
                        {a.type} · {a.version}
                      </small>
                    </div>
                  ))}
                </WorkSection>
                {session.error && (
                  <WorkNotice title="Session error" tone="error">
                    {session.error}
                  </WorkNotice>
                )}
              </aside>
            </div>
          </>
        ) : (
          <WorkEmpty
            title="No investigation open"
            action={
              <button
                className="work-button"
                onClick={() => setWorkspace("prepare")}
              >
                Prepare context
              </button>
            }
          >
            Choose artifacts and start a run, or reopen a recent session.
          </WorkEmpty>
        ))}
      {workspace === "history" && (
        <WorkSection
          title="Recent live sessions"
          description="Reopen the server-side stream and inspect its recorded evidence."
        >
          <div className="work-table-wrap">
            <table className="work-table">
              <thead>
                <tr>
                  <th>Session</th>
                  <th>State</th>
                  <th>Agent</th>
                  <th>Updated</th>
                  <th>Action</th>
                </tr>
              </thead>
              <tbody>
                {sessions.map((s) => (
                  <tr key={s.id}>
                    <td>
                      <button className="record-title" onClick={() => open(s)}>
                        {s.title || "Untitled investigation"}
                      </button>
                      <small>{s.id}</small>
                    </td>
                    <td>{s.state}</td>
                    <td>{s.agent}</td>
                    <td>{new Date(s.updated_at).toLocaleString()}</td>
                    <td>
                      <button
                        className="work-button danger"
                        onClick={() => void remove(s.id)}
                      >
                        Remove session
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!sessions.length && (
              <WorkEmpty title="No live sessions yet">
                Start an investigation with selected artifacts.
              </WorkEmpty>
            )}
          </div>
        </WorkSection>
      )}
    </WorkPage>
  );
}
