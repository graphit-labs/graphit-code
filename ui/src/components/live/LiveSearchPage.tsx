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
import type { AgentProgress } from "@/api/agentStream";
import { stringifyNativeJson } from "@/api/nativeJson";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import {
  cancelLiveTurn,
  createLiveSession,
  getLiveSession,
  closeLiveSession,
  listLiveSessions,
  removeLiveSession,
  sendLiveMessage,
  subscribeLiveEvents,
  LiveError,
  type LiveArtifactSelection,
  type LiveProjectSelection,
  type LiveEvent,
  type LiveSession,
  type LiveSubscription,
  type LiveCursor,
  type LiveTerminal,
  type LiveOutcome,
} from "@/api/live";
import { loadLiveCatalog, filterLiveCatalog, artifactKey, projectKey, type LiveCatalogBinding, type LiveCatalogEntry } from "@/api/liveCatalog";
import { LoadingSpinner } from "@/components/shared/LoadingSpinner";
import { useAppStore } from "@/store/appStore";

interface Turn {
  id: string | null;
  question?: string;
  answer: string;
  activity: AgentProgress[];
  currentEvent?: AgentProgress;
  errors: string[];
  done: boolean;
  outcome: LiveOutcome | 'idle';
}

export function transcriptFromEvents(events: LiveEvent[]): Turn[] {
  const turns: Turn[] = [];
  const byId = new Map<string | null, Turn>();
  for (const ev of events) {
    const data = ev.data;
    if (data.type === 'state') continue;
    let turn = byId.get(ev.turn_id);
    if (!turn) {
      turn = { id: ev.turn_id, answer: '', activity: [], errors: [], done: false, outcome: 'idle' };
      byId.set(ev.turn_id, turn); turns.push(turn);
    }
    if (data.type === 'prompt') { turn.question = data.text; continue; }
    if (data.type === 'error') { turn.errors.push(data.code); continue; }
    if (data.type === 'turn_done') { turn.done = true; turn.outcome = data.outcome; continue; }
    const progress: AgentProgress = { kind: data.type };
    if ('text' in data) progress.text = data.text;
    if (data.type === 'prep') progress.text = data.message;
    if (data.type === 'tool_use') {
      progress.tool_call_id = data.tool_call_id; progress.tool = data.name || data.title;
      progress.detail = typeof data.input === 'string' ? data.input : stringifyNativeJson(data.input, 2);
    }
    if (data.type === 'tool_result') {
      progress.tool_call_id = data.tool_call_id;
      progress.detail = (data.is_error ? 'Tool error:\n' : '') + (typeof data.output === 'string' ? data.output : stringifyNativeJson(data.output, 2));
    }
    if (data.type === 'text') turn.answer += data.text;
    else turn.activity.push(progress);
    if (executionEventLabel(progress)) turn.currentEvent = progress;
  }
  return turns;
}

function executionProblem(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  return error instanceof LiveError && error.journalEffect ? message + ' · History effect: ' + error.journalEffect : message;
}
const catalogueOrigin = (entry: LiveCatalogEntry) => [entry.sourceLabel, entry.project_name, entry.ref.project_id, entry.ref.instance_id, entry.ref.project_kind].filter(Boolean).join(' · ');

export default function LiveSearchPage() {
  const { activeProjectDir, activeAgent } = useAppStore();

  const [entries, setEntries] = useState<LiveCatalogEntry[]>([]);
  const [catalogErrors, setCatalogErrors] = useState<string[]>([]);
  const [catalogLoading, setCatalogLoading] = useState(false);
  const [sourceFilter, setSourceFilter] = useState("");
  const [chosen, setChosen] = useState<LiveArtifactSelection[]>([]);
  const [projects, setProjects] = useState<LiveProjectSelection[]>([]);
  const [bindings, setBindings] = useState<LiveCatalogBinding[]>([]);
  const [projectChoice, setProjectChoice] = useState('');
  const [bindingChoice, setBindingChoice] = useState('');
  const [agentChoice, setAgentChoice] = useState('');
  const [model, setModel] = useState('');
  const [typeFilter, setTypeFilter] = useState("");
  const [pickerQuery, setPickerQuery] = useState("");

  const [question, setQuestion] = useState("");
  const [session, setSession] = useState<LiveSession | null>(null);
  const [events, setEvents] = useState<LiveEvent[]>([]);
  const [sessions, setSessions] = useState<LiveSession[]>([]);
  const [starting, setStarting] = useState(false);
  const [sending, setSending] = useState(false);
  const [awaitingTurn, setAwaitingTurn] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [sessionsProblem, setSessionsProblem] = useState<string | null>(null);
  const [streamQuiet, setStreamQuiet] = useState(false);
  const [terminal, setTerminal] = useState<LiveTerminal | null>(null);
  const [opening, setOpening] = useState(false);
  const [closing, setClosing] = useState(false);
  const [workspace, setWorkspace] = useState("prepare");
  const [evidence, setEvidence] = useState("output");

  const subscription = useRef<LiveSubscription | null>(null);
  const replayCursor = useRef<LiveCursor | null>(null);
  const detailRequest = useRef<AbortController | null>(null);
  const sessionRequest = useRef(0);
  const sessionObservation = useRef(0);
  const sessionId = useRef<string | null>(null);
  const createAttempt = useRef<{ body: string; requestId: string } | null>(null);
  const sendAttempt = useRef<{ sessionId: string; text: string; turnId: string } | null>(null);
  useLayoutEffect(() => { sessionId.current = session?.id ?? null; }, [session?.id]);
  const transcriptEnd = useRef<HTMLDivElement | null>(null);

  const refreshSessions = useCallback(() => {
    return listLiveSessions()
      .then(rows => { setSessions(rows); setSessionsProblem(null); })
      .catch((error) => { setSessionsProblem(`Could not refresh recent sessions: ${error instanceof Error ? error.message : String(error)}`); });
  }, []);

  const registryRequest = useRef(0);
  const catalogRequest = useRef<AbortController | null>(null);
  const activeScope = `${activeProjectDir}\0${activeAgent}`;
  const activeScopeRef = useRef(activeScope); useLayoutEffect(() => { activeScopeRef.current = activeScope; createAttempt.current = null; }, [activeScope]);
  const loadArtifacts = useCallback(async () => {
    const request = ++registryRequest.current;
    catalogRequest.current?.abort();
    const abort = new AbortController(); catalogRequest.current = abort;
    setCatalogLoading(true);
    try {
      const result = await loadLiveCatalog(abort.signal);
      if (!abort.signal.aborted && request === registryRequest.current) {
        setEntries(result.entries); setCatalogErrors(result.errors);
        setProjects(result.projects); setBindings(result.bindings);
        const available=new Set(result.entries.filter(e=>!e.unavailable).map(e=>artifactKey(e.ref)));
        setChosen(items=>items.filter(item=>available.has(artifactKey(item))));
        setAgentChoice(value => available.has(value) ? value : '');
        setProjectChoice(value => result.projects.some(project => projectKey(project) === value) ? value : '');
        setBindingChoice(value => result.bindings.some(binding => binding.binding === value && binding.available) ? value : '');
      }
    } catch (error) {
      if (!abort.signal.aborted && request === registryRequest.current) {
        setCatalogErrors([error instanceof Error ? error.message : String(error)]);
        setEntries([]); setProjects([]); setBindings([]); setChosen([]); setAgentChoice(''); setProjectChoice(''); setBindingChoice('');
      }
    } finally { if (!abort.signal.aborted && request === registryRequest.current) setCatalogLoading(false); }
  }, []);
  const [catalogScope, setCatalogScope] = useState(activeScope);
  if (catalogScope !== activeScope) {
    setCatalogScope(activeScope); setChosen([]); setEntries([]); setCatalogErrors([]);
    setProjects([]); setBindings([]); setProjectChoice(''); setBindingChoice(''); setAgentChoice('');
  }
  useEffect(() => {
    let active = true;
    queueMicrotask(() => { if (active) void loadArtifacts(); });
    void refreshSessions();
    return () => { active = false; registryRequest.current++; catalogRequest.current?.abort(); };
  }, [activeScope, loadArtifacts, refreshSessions]);
  usePageRefresh(() => refreshAll([loadArtifacts(), refreshSessions()]));

  useEffect(() => {
    subscription.current?.close();
    subscription.current = null;
    if (!session) return;

    let active = true;
    let snapshotHead = session.cursor.sequence;
    if (session.state === 'closed') return;
    const sub = subscribeLiveEvents(session.id, replayCursor.current ?? session.cursor, {
      onEvent: (ev) => {
        if (!active) return;
        setEvents((prev) =>
          prev.some((p) => p.sequence === ev.sequence) ? prev : [...prev, ev],
        );
        if (ev.data.type === 'turn_done') setAwaitingTurn(value => value === ev.turn_id ? null : value);
        // Replayed events belong in the transcript, but cannot regress the opening snapshot.
        if (BigInt(ev.sequence) <= BigInt(snapshotHead)) return;
        if ((ev.data.type === 'prompt' || ev.data.type === 'turn_done') && ev.turn_id !== null) setTerminal(value => value?.turn_id === ev.turn_id ? value : null);
        if (ev.data.type === "state") {
          const state = ev.data.state;
          sessionObservation.current++;
          setSession((s) => (s ? { ...s, state } : s));
          if (state === 'running' && ev.turn_id !== null) setTerminal(value => value?.turn_id === ev.turn_id ? value : null);
          if (state === "ready" || state === "failed") void refreshSessions();
        }
      },
      onOpen: (value) => { if (active) {
        sessionObservation.current++;
        snapshotHead = value.session.cursor.sequence;
        setStreamQuiet(false); setSession(value.session); setTerminal(value.terminal);
        if (value.terminal) setAwaitingTurn(turn => turn === value.terminal?.turn_id ? null : turn);
      } },
      onTerminal: (value) => { if (active) { setTerminal(value); setAwaitingTurn(turn => turn === value.turn_id ? null : turn); void refreshSessions(); } },
      onClosed: (value) => { if (active) { sessionObservation.current++; setSession(value); setAwaitingTurn(null); setStreamQuiet(false); void refreshSessions(); } },
      onError: (error) => { if (active) { setStreamQuiet(!error); if (error) setProblem(executionProblem(error)); } },
    });
    subscription.current = sub;
    return () => {
      active = false;
      sub.close();
    };
  }, [session?.id, refreshSessions]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => { ++sessionRequest.current; detailRequest.current?.abort(); }, []);

  const resetSession = () => {
    ++sessionRequest.current; detailRequest.current?.abort(); subscription.current?.close();
    setSession(null); setEvents([]); setTerminal(null); setOpening(false); setClosing(false); setStreamQuiet(false);
    replayCursor.current = null;
    createAttempt.current = null; sendAttempt.current = null; setSending(false); setAwaitingTurn(null);
  };

  const turns = useMemo(() => transcriptFromEvents(events), [events]);

  useEffect(() => {
    if (workspace === "run" && evidence === "output") transcriptEnd.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [turns]);

  const types = useMemo(() => {
    const seen = new Set<string>();
    for (const e of entries) if (e.type && e.type !== 'agent') seen.add(e.type);
    return Array.from(seen).sort();
  }, [entries]);

  const visibleEntries = useMemo(() => {
    return filterLiveCatalog(entries.filter(entry => entry.type !== 'agent'),pickerQuery,typeFilter,sourceFilter);
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
    if (!canStart || !selectedProject || !selectedAgent || !selectedBinding) return;
    setProblem(null);
    setStarting(true);
    const request = ++sessionRequest.current;
    const startScope=activeScopeRef.current;
    try {
      const input = { project: selectedProject, binding: selectedBinding.binding, agent: selectedAgent.ref, artifacts: chosen, model: model.trim(), prompt: question.trim() || undefined };
      const body = stringifyNativeJson(input);
      if (createAttempt.current?.body !== body) createAttempt.current = { body, requestId: crypto.randomUUID() };
      const requestId = createAttempt.current.requestId;
      const created = await createLiveSession({
        ...input, request_id: requestId,
      });
      if(startScope!==activeScopeRef.current || request !== sessionRequest.current) {void refreshSessions();return;}
      setEvents([]);
      setTerminal(null);
      setAwaitingTurn(input.prompt && created.state !== 'closed' ? requestId : null);
      replayCursor.current = { generation: created.cursor.generation, sequence: '0' };
      setSession(created);
      setWorkspace("run");
      setQuestion("");
      createAttempt.current = null;
      refreshSessions();
    } catch (e) {
      if(startScope===activeScopeRef.current && request === sessionRequest.current) setProblem(executionProblem(e));
    } finally {
      setStarting(false);
    }
  };

  const ask = async () => {
    if (!session || !canAsk) return;
    const id = session.id;
    const request = sessionRequest.current;
    const observation = sessionObservation.current;
    setProblem(null);
    const prompt = question;
    if (sendAttempt.current?.sessionId !== id || sendAttempt.current.text !== prompt) sendAttempt.current = { sessionId: id, text: prompt, turnId: crypto.randomUUID() };
    const turnId = sendAttempt.current.turnId;
    setSending(true);
    setAwaitingTurn(turnId);
    setQuestion("");
    try {
      const accepted = await sendLiveMessage(id, { turn_id: turnId, text: prompt });
      if (sessionId.current === id && request === sessionRequest.current) {
        const unchanged = sessionObservation.current === observation;
        setSession((s) => (unchanged && s?.id === id && s.state !== 'closed' ? accepted : s));
        setTerminal(value => value?.turn_id === turnId ? value : null);
        if (accepted.state === 'closed') setAwaitingTurn(value => value === turnId ? null : value);
      }
      if (sendAttempt.current?.turnId === turnId) sendAttempt.current = null;
    } catch (e) {
      if (sessionId.current === id && request === sessionRequest.current) { setAwaitingTurn(value => value === turnId ? null : value); setProblem(executionProblem(e)); setQuestion(current => current || prompt); }
    } finally { if (sessionId.current === id && request === sessionRequest.current) setSending(false); }
  };

  const stop = async () => {
    if (!session) return;
    const id = session.id;
    try {
      await cancelLiveTurn(id);
    } catch (e) {
      if (sessionId.current === id) setProblem(executionProblem(e));
    }
  };

  const closeSession = async () => {
    if (!session || closing) return;
    const id = session.id;
    setClosing(true); setProblem(null);
    try {
      const closed = await closeLiveSession(id);
      if (sessionId.current === id) { subscription.current?.close(); setSession(closed); setAwaitingTurn(null); setStreamQuiet(false); }
      void refreshSessions();
    } catch (error) { if (sessionId.current === id) setProblem(executionProblem(error)); }
    finally { if (sessionId.current === id) setClosing(false); }
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
      if (sessionId.current === id) resetSession();
      refreshSessions();
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e));
    }
  };

  const open = async (s: LiveSession) => {
    resetSession();
    const request = sessionRequest.current;
    const abort = new AbortController(); detailRequest.current = abort;
    setOpening(true); setProblem(null);
    setWorkspace("run");
    try {
      const detail = await getLiveSession(s.id, abort.signal);
      if (abort.signal.aborted || request !== sessionRequest.current) return;
      replayCursor.current = detail.session.cursor;
      setEvents(detail.events); setTerminal(detail.terminal); setSession(detail.session);
    } catch (error) {
      if (!abort.signal.aborted && request === sessionRequest.current) setProblem(error instanceof Error ? error.message : String(error));
    } finally { if (request === sessionRequest.current) setOpening(false); }
  };

  const currentTerminal = terminal && (!awaitingTurn || terminal.turn_id === awaitingTurn) ? terminal : null;
  const busy = (session?.state === "preparing" || session?.state === "running") && !currentTerminal?.joined;
  const selectedProject = projects.find(project => projectKey(project) === projectChoice);
  const selectedBinding = bindings.find(binding => binding.binding === bindingChoice && binding.available);
  const agents = entries.filter(entry => entry.type === 'agent');
  const selectedAgent = agents.find(entry => artifactKey(entry.ref) === agentChoice && !entry.unavailable);
  const canStart = !starting && !catalogLoading && !!selectedProject && !!selectedBinding && !!selectedAgent && model.trim().length > 0;
  const canAsk = !!session && session.state !== 'closed' && (session.state === 'ready' || !!(currentTerminal?.joined && currentTerminal.clean)) && !closing && !sending && !awaitingTurn && question.trim().length > 0;

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
          disabled={starting}
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
              } else if (canStart)
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
            {sending ? 'Sending…' : 'Ask'}
          </button>
        ) : (
          <button
            className="work-button primary"
            disabled={!canStart}
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
              resetSession();
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
              {catalogLoading && <p role="status">Loading available projects, bindings and artifacts…</p>}
              {catalogErrors.map(message=><WorkNotice key={message} title="Some sources could not load" tone="error">{message}</WorkNotice>)}
              {!!catalogErrors.length && <button className="work-button" disabled={catalogLoading} onClick={() => void loadArtifacts()}>Refresh catalogue</button>}
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
                            disabled={starting || !!e.unavailable}
                            onChange={() => toggle(e)}
                          />
                        </td>
                        <td>
                          <strong>{e.name}</strong>
                          {e.description && <small>{e.description}</small>}
                          <small>{e.id}</small>
                          <small>{catalogueOrigin(e)}</small>
                          {e.unavailable && <small>{e.unavailable}</small>}
                        </td>
                        <td>{e.type}</td>
                        <td>{e.ref.version}</td>
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
              <div className="work-form">
                <label className="work-field"><span>Project and version</span><StyledSelect aria-label="Investigation project" value={projectChoice} disabled={starting || catalogLoading} onChange={event => setProjectChoice(event.target.value)}>
                  <option value="">Choose a project and version</option>
                  {projects.map(project => <option key={projectKey(project)} value={projectKey(project)}>{project.project_id} · {project.project_version}</option>)}
                </StyledSelect></label>
                <label className="work-field"><span>Execution binding</span><StyledSelect aria-label="Execution binding" value={bindingChoice} disabled={starting || catalogLoading} onChange={event => setBindingChoice(event.target.value)}>
                  <option value="">Choose a binding</option>
                  {bindings.map(binding => <option key={binding.binding} value={binding.binding} disabled={!binding.available}>{binding.binding}{!binding.available ? ' — unavailable' : ''}</option>)}
                </StyledSelect></label>
                <label className="work-field"><span>Installed agent</span><StyledSelect aria-label="Installed agent" value={agentChoice} disabled={starting || catalogLoading} onChange={event => setAgentChoice(event.target.value)}>
                  <option value="">Choose an installed agent</option>
                  {agents.map(agent => <option key={artifactKey(agent.ref)} value={artifactKey(agent.ref)} disabled={!!agent.unavailable}>{agent.name} · {agent.ref.version} · {catalogueOrigin(agent)}{agent.unavailable ? ' — ' + agent.unavailable : ''}</option>)}
                </StyledSelect></label>
                <label className="work-field"><span>Model</span><input aria-label="Model" value={model} required disabled={starting} onChange={event => setModel(event.target.value)} placeholder="Model identifier" /></label>
              </div>
              <FactList
                items={[
                  ["Agent", selectedAgent?.name || "Choose an agent"],
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
                      resetSession();
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
              {session.state !== 'closed' && <button className="work-button" disabled={closing} onClick={() => void closeSession()}>{closing ? 'Closing…' : 'Close session'}</button>}
              {session.state === "running" && busy && (
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
            {awaitingTurn && !sending && session.state !== 'running' && <p role="status">Waiting for turn updates…</p>}
            {currentTerminal && <WorkSection title="Observed turn completion"><FactList items={[
              ['Turn', currentTerminal.turn_id], ['Outcome', currentTerminal.outcome], ['Joined', currentTerminal.joined ? 'Yes' : 'No'],
              ['Cleanup', currentTerminal.clean ? 'Complete' : 'Incomplete'], ['History saved', currentTerminal.persisted ? 'Yes' : 'No'],
              ...(currentTerminal.error ? [['Error', currentTerminal.error] as [string, string]] : []),
            ]} /></WorkSection>}
            <div className="live-run-layout">
              <section className="work-panel">
                <LiveEvidence key={stringifyNativeJson([session.id, session.cursor.generation])} sessionId={session.id} generation={session.cursor.generation} value={evidence} onChange={setEvidence}
                  activity={<div>{turns.map((turn, index) => <section key={turn.id ?? 'preparation'} aria-label={'Request ' + (index + 1) + ' activity'}><ExecutionActivity events={turn.activity} /></section>)}</div>}
                  output={onLink => (<div className="live-transcript">
                    {turns.map((t, i) => (
                      <article key={i}>
                        <header>
                          <small>Request {i + 1}</small>
                          <h3>{t.question || "Agent preparation"}</h3>
                        </header>
                        <AgentExecution events={t.activity} currentEvent={t.currentEvent} running={!t.done && busy} outcome={t.outcome} />
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
                            ? t.outcome === 'cancelled' ? "Turn stopped" : t.outcome === 'failed' ? "Turn failed" : "Turn complete"
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
                      ["Agent", session.agent.id],
                      ['Agent version', session.agent.version],
                      ['Model', session.model],
                      ['Project', session.project.project_id],
                      ['Project version', session.project.project_version],
                      ['Binding', session.binding],
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
                      key={stringifyNativeJson([a.source, a.project_id, a.instance_id, a.project_kind, a.type, a.id, a.version])}
                    >
                      <strong className="break-all">{a.id}</strong>
                      <small className="block mt-2 text-muted-foreground">
                        {a.type} · {a.version}
                      </small>
                    </div>
                  ))}
                </WorkSection>
              </aside>
            </div>
          </>
        ) : (
          opening ? <LoadingSpinner label="Opening live history…" /> : <WorkEmpty
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
                      <button className="record-title" onClick={() => void open(s)}>
                        {s.title || "Untitled investigation"}
                      </button>
                      <small>{s.id}</small>
                    </td>
                    <td>{s.state}</td>
                    <td>{s.agent.id}</td>
                    <td>{s.updated_at}</td>
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
