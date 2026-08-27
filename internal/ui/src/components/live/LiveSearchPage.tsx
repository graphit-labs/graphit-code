import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
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
} from 'lucide-react'
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
} from '@/api/live'
import { hubApi, type RegistryEntry } from '@/api/hub'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { useAppStore } from '@/store/appStore'
import { cn } from '@/lib/utils'

/**
 * The live search page watches a session; it does not perform a search.
 *
 * Everything expensive happens on the server and outlives this component: the
 * throwaway project is built, the artifacts are installed and indexed, and an agent
 * runs for as long as it takes. So the page's whole job is to choose what to search,
 * then render an event stream — and to be closable at any moment without consequence.
 */

interface Turn {
  question?: string
  answer: string
  activity: Array<{ seq: number; label: string; detail?: string }>
  errors: string[]
  done: boolean
}

function transcriptFromEvents(events: LiveEvent[]): Turn[] {
  const turns: Turn[] = []
  const current = (): Turn => {
    if (turns.length === 0 || turns[turns.length - 1].done) {
      turns.push({ answer: '', activity: [], errors: [], done: false })
    }
    return turns[turns.length - 1]
  }

  for (const ev of events) {
    switch (ev.kind) {
      case 'prompt': {
        const t = current()
        if (t.question !== undefined) {
          turns.push({ question: ev.text, answer: '', activity: [], errors: [], done: false })
        } else {
          t.question = ev.text
        }
        break
      }
      case 'text':
        current().answer += ev.text ?? ''
        break
      case 'prep':
        current().activity.push({ seq: ev.seq, label: ev.text ?? '' })
        break
      case 'tool_use':
        current().activity.push({ seq: ev.seq, label: ev.tool ?? 'tool', detail: ev.detail })
        break
      case 'error':
        current().errors.push(ev.text ?? '')
        break
      case 'turn_done':
        current().done = true
        break
      default:
        break
    }
  }
  return turns
}

type LiveState = LiveSession['state']

function stateTone(state: LiveState | undefined): string {
  switch (state) {
    case 'ready':
      return 'text-emerald-600 dark:text-emerald-400 bg-emerald-500/10 border-emerald-500/30'
    case 'running':
      return 'text-primary bg-primary/10 border-primary/30'
    case 'preparing':
      return 'text-amber-600 dark:text-amber-400 bg-amber-500/10 border-amber-500/30'
    case 'failed':
      return 'text-red-600 dark:text-red-400 bg-red-500/10 border-red-500/30'
    default:
      return 'text-muted-foreground bg-muted/40 border-border/40'
  }
}

function typeBadgeStyle(type: string): string {
  switch (type.toLowerCase()) {
    case 'knowledge':
      return 'bg-sky-500/10 text-sky-600 dark:text-sky-400 border-sky-500/30'
    case 'skill':
      return 'bg-purple-500/10 text-purple-600 dark:text-purple-400 border-purple-500/30'
    case 'agent':
      return 'bg-amber-500/10 text-amber-600 dark:text-amber-400 border-amber-500/30'
    case 'rule':
      return 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/30'
    case 'ast':
      return 'bg-indigo-500/10 text-indigo-600 dark:text-indigo-400 border-indigo-500/30'
    case 'mcp':
      return 'bg-rose-500/10 text-rose-600 dark:text-rose-400 border-rose-500/30'
    case 'command':
      return 'bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border-cyan-500/30'
    default:
      return 'bg-muted/50 text-muted-foreground border-border/40'
  }
}

export default function LiveSearchPage() {
  const { activeProjectDir, activeIde, projectsLoaded, loadProjects } = useAppStore()

  const [entries, setEntries] = useState<RegistryEntry[]>([])
  const [chosen, setChosen] = useState<LiveArtifact[]>([])
  const [typeFilter, setTypeFilter] = useState('')
  const [pickerQuery, setPickerQuery] = useState('')

  const [question, setQuestion] = useState('')
  const [session, setSession] = useState<LiveSession | null>(null)
  const [events, setEvents] = useState<LiveEvent[]>([])
  const [sessions, setSessions] = useState<LiveSession[]>([])
  const [starting, setStarting] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)
  const [streamQuiet, setStreamQuiet] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)

  const subscription = useRef<LiveSubscription | null>(null)
  const transcriptEnd = useRef<HTMLDivElement | null>(null)

  const refreshSessions = useCallback(() => {
    listLiveSessions().then(setSessions).catch(() => { /* non blocking */ })
  }, [])

  useEffect(() => {
    hubApi.getRegistry(activeProjectDir || undefined)
      .then(r => setEntries(r.entries ?? []))
      .catch(() => setEntries([]))
    if (!projectsLoaded) void loadProjects()
    refreshSessions()
  }, [activeProjectDir, projectsLoaded, loadProjects, refreshSessions])

  useEffect(() => {
    subscription.current?.close()
    subscription.current = null
    if (!session) return

    const sub = subscribeLiveEvents(session.id, 0, {
      onEvent: ev => {
        setEvents(prev => (prev.some(p => p.seq === ev.seq) ? prev : [...prev, ev]))
        if (ev.kind === 'state' && ev.state) {
          setSession(s => (s ? { ...s, state: ev.state as LiveState } : s))
          if (ev.state === 'ready' || ev.state === 'failed') refreshSessions()
        }
      },
      onOpen: () => setStreamQuiet(false),
      onError: () => setStreamQuiet(true),
    })
    subscription.current = sub
    return () => { sub.close() }
  }, [session?.id, refreshSessions]) // eslint-disable-line react-hooks/exhaustive-deps

  const turns = useMemo(() => transcriptFromEvents(events), [events])

  useEffect(() => {
    transcriptEnd.current?.scrollIntoView({ behavior: 'smooth', block: 'end' })
  }, [turns])

  const types = useMemo(() => {
    const seen = new Set<string>()
    for (const e of entries) if (e.type) seen.add(e.type)
    return Array.from(seen).sort()
  }, [entries])

  const visibleEntries = useMemo(() => {
    const q = pickerQuery.trim().toLowerCase()
    return entries.filter(e => {
      if (typeFilter && e.type !== typeFilter) return false
      if (!q) return true
      return `${e.id} ${e.name ?? ''} ${e.description ?? ''}`.toLowerCase().includes(q)
    })
  }, [entries, typeFilter, pickerQuery])

  const isChosen = (e: RegistryEntry) => chosen.some(c => c.id === e.id && c.type === e.type)

  const toggle = (e: RegistryEntry) => {
    setChosen(prev => prev.some(c => c.id === e.id && c.type === e.type)
      ? prev.filter(c => !(c.id === e.id && c.type === e.type))
      : [...prev, { id: e.id, type: e.type, version: e.latest }])
  }

  const start = async () => {
    setProblem(null)
    setStarting(true)
    try {
      const created = await createLiveSession({
        ide: activeIde,
        artifacts: chosen,
        prompt: question.trim() || undefined,
      })
      setEvents([])
      setSession(created)
      setQuestion('')
      refreshSessions()
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e))
    } finally {
      setStarting(false)
    }
  }

  const ask = async () => {
    if (!session || !question.trim()) return
    setProblem(null)
    const prompt = question
    setQuestion('')
    try {
      await sendLiveMessage(session.id, prompt)
      setSession(s => (s ? { ...s, state: 'running' } : s))
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e))
      setQuestion(prompt)
    }
  }

  const stop = async () => {
    if (!session) return
    try { await cancelLiveTurn(session.id) } catch { /* already finished */ }
  }

  const remove = async (id: string) => {
    try {
      await removeLiveSession(id)
      if (session?.id === id) {
        setSession(null)
        setEvents([])
      }
      refreshSessions()
    } catch (e) {
      setProblem(e instanceof Error ? e.message : String(e))
    }
  }

  const open = (s: LiveSession) => {
    setEvents([])
    setSession(s)
  }

  const busy = session?.state === 'preparing' || session?.state === 'running'
  const canAsk = session?.state === 'ready' && question.trim().length > 0

  return (
    <div className="w-full max-w-7xl mx-auto px-4 md:px-8 py-8 animate-in fade-in duration-300">
      {/* Top Banner Header */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 mb-6 pb-6 border-b border-border/40">
        <div className="flex items-center gap-4">
          <div className="w-12 h-12 rounded-2xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0 shadow-sm">
            <Radio className={cn('w-6 h-6 text-primary', busy && 'animate-pulse')} />
          </div>
          <div>
            <h1 className="text-3xl font-heading font-bold tracking-tight text-foreground">Live Search</h1>
            <p className="text-sm text-muted-foreground mt-1">
              Streamed autonomous agent running on throwaway projects with your selected artifacts
            </p>
            <div className="flex items-center gap-2 mt-2.5 glass-pill px-3 py-1.5 w-fit">
              <Monitor className="w-3.5 h-3.5 text-muted-foreground shrink-0" />
              <span className="text-xs text-muted-foreground uppercase tracking-widest font-semibold">Target IDE:</span>
              <span className="text-xs font-mono font-semibold text-primary">{activeIde || '…'}</span>
            </div>
          </div>
        </div>

        {session && (
          <div className="flex items-center gap-2.5 flex-wrap">
            <span className={cn('text-xs font-bold uppercase px-3 py-1.5 rounded-xl border shadow-sm', stateTone(session.state))}>
              {session.state}
            </span>
            {session.state === 'running' && (
              <button
                onClick={stop}
                className="flex items-center gap-1.5 text-xs font-semibold px-3 py-2 rounded-xl bg-destructive/10 hover:bg-destructive/20 text-destructive border border-destructive/30 transition-all hover:scale-[1.02]"
              >
                <Square className="w-3.5 h-3.5" /> Stop Run
              </button>
            )}
            <button
              onClick={() => { setSession(null); setEvents([]); }}
              className="flex items-center gap-1.5 text-xs font-semibold px-3.5 py-2 rounded-xl border border-border/50 hover:bg-accent/50 glass-panel transition-all hover:scale-[1.02]"
            >
              <Plus className="w-3.5 h-3.5 text-primary" /> New Search
            </button>
          </div>
        )}
      </div>

      {/* Main Flex Layout: Sidebar (Collapsible) + Console (Expands smoothly) */}
      <div className="flex gap-6 h-[calc(100vh-250px)] min-h-[580px] transition-all duration-300">
        {/* Left Sidebar Pane */}
        <aside
          className={cn(
            'flex flex-col min-h-0 glass-panel rounded-2xl border border-border/40 overflow-hidden shadow-sm transition-all duration-300 ease-in-out shrink-0',
            sidebarCollapsed ? 'w-16 p-2 items-center' : 'w-80 lg:w-96 p-4',
          )}
        >
          {sidebarCollapsed ? (
            /* Collapsed Sidebar Strip */
            <div className="flex flex-col items-center gap-4 py-2 w-full">
              <button
                onClick={() => setSidebarCollapsed(false)}
                className="p-2 rounded-xl bg-primary/10 border border-primary/30 text-primary hover:bg-primary/20 transition-all"
                title="Expand sidebar"
              >
                <PanelLeftOpen className="w-4 h-4" />
              </button>
              <div className="w-full border-t border-border/40 my-1" />

              <div
                className="relative p-2.5 rounded-xl bg-card/50 border border-border/40 text-muted-foreground hover:text-foreground cursor-pointer transition-colors"
                onClick={() => setSidebarCollapsed(false)}
                title={`Target Artifacts (${chosen.length} selected)`}
              >
                <Layers className="w-4 h-4 text-primary" />
                {chosen.length > 0 && (
                  <span className="absolute -top-1 -right-1 bg-primary text-primary-foreground text-[9px] font-bold w-4 h-4 rounded-full flex items-center justify-center">
                    {chosen.length}
                  </span>
                )}
              </div>

              <div
                className="relative p-2.5 rounded-xl bg-card/50 border border-border/40 text-muted-foreground hover:text-foreground cursor-pointer transition-colors"
                onClick={() => setSidebarCollapsed(false)}
                title={`Recent Sessions (${sessions.length})`}
              >
                <MessageSquare className="w-4 h-4 text-primary" />
                {sessions.length > 0 && (
                  <span className="absolute -top-1 -right-1 bg-accent text-foreground text-[9px] font-bold w-4 h-4 rounded-full flex items-center justify-center border border-border/40">
                    {sessions.length}
                  </span>
                )}
              </div>
            </div>
          ) : (
            /* Full Expanded Sidebar */
            <div className="flex-1 flex flex-col min-h-0 overflow-hidden">
              <div className="flex items-center justify-between pb-3 mb-3 border-b border-border/30">
                <span className="text-xs font-bold uppercase tracking-wider text-muted-foreground">Configuration & Artifacts</span>
                <button
                  onClick={() => setSidebarCollapsed(true)}
                  className="p-1.5 rounded-lg border border-border/40 text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-all"
                  title="Collapse sidebar (expand chat)"
                >
                  <PanelLeftClose className="w-3.5 h-3.5" />
                </button>
              </div>

              <div className="flex-1 overflow-y-auto space-y-6 pr-1 scrollbar-thin">
                {/* Agent Conventions Info */}
                <div className="bg-card/40 border border-border/30 rounded-xl p-3.5 space-y-2">
                  <div className="flex items-center gap-2 text-xs font-semibold text-foreground">
                    <Monitor className="w-4 h-4 text-primary shrink-0" />
                    <span>Agent Conventions</span>
                  </div>
                  <p className="text-xs text-muted-foreground leading-relaxed">
                    Rules, skills, and MCP setup for the throwaway project follow your selected IDE: <strong className="text-foreground">{activeIde || 'Default'}</strong>. Change it in the project switcher above.
                  </p>
                </div>

                {/* Artifact Picker */}
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <label className="text-xs font-bold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5">
                      <Layers className="w-3.5 h-3.5 text-primary" />
                      <span>Target Artifacts ({chosen.length})</span>
                    </label>
                    {chosen.length > 0 && (
                      <button
                        onClick={() => setChosen([])}
                        className="text-[11px] text-muted-foreground hover:text-foreground transition-colors"
                      >
                        Clear selection
                      </button>
                    )}
                  </div>

                  {/* Filter controls */}
                  <div className="flex gap-2">
                    <div className="relative flex-1">
                      <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground pointer-events-none" />
                      <input
                        value={pickerQuery}
                        onChange={e => setPickerQuery(e.target.value)}
                        placeholder="Filter artifacts..."
                        className="w-full pl-8 pr-3 py-1.5 text-xs rounded-xl bg-background/50 border border-border/50 backdrop-blur-sm outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all"
                      />
                    </div>
                    <select
                      value={typeFilter}
                      onChange={e => setTypeFilter(e.target.value)}
                      className="text-xs rounded-xl bg-background/50 border border-border/50 px-2 py-1.5 outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all"
                    >
                      <option value="">All Types</option>
                      {types.map(t => <option key={t} value={t}>{t}</option>)}
                    </select>
                  </div>

                  {/* Artifact list */}
                  <div className="space-y-1.5 max-h-60 overflow-y-auto pr-1">
                    {visibleEntries.length === 0 ? (
                      <p className="text-xs text-muted-foreground/70 py-4 text-center">
                        No registry artifacts match your search.
                      </p>
                    ) : (
                      visibleEntries.map(e => {
                        const selected = isChosen(e)
                        return (
                          <button
                            key={`${e.type}:${e.id}`}
                            onClick={() => toggle(e)}
                            className={cn(
                              'w-full text-left rounded-xl p-2.5 border transition-all duration-200 glass-panel-hover flex items-start gap-2.5 group',
                              selected
                                ? 'bg-primary/10 border-primary/40 shadow-sm'
                                : 'bg-card/30 border-border/30 hover:bg-accent/40',
                            )}
                          >
                            <div className={cn(
                              'w-4 h-4 rounded-md border flex items-center justify-center shrink-0 mt-0.5 transition-colors',
                              selected
                                ? 'bg-primary border-primary text-primary-foreground'
                                : 'border-border/60 group-hover:border-primary/50',
                            )}>
                              {selected && <Check className="w-3 h-3 stroke-[3]" />}
                            </div>
                            <div className="flex-1 min-w-0">
                              <div className="flex items-center justify-between gap-1.5">
                                <span className="text-xs font-semibold text-foreground truncate">{e.name || e.id}</span>
                                <span className={cn('text-[9px] font-bold uppercase px-1.5 py-0.5 rounded border shrink-0', typeBadgeStyle(e.type))}>
                                  {e.type}
                                </span>
                              </div>
                              {e.description && (
                                <p className="text-[11px] text-muted-foreground truncate mt-0.5">{e.description}</p>
                              )}
                            </div>
                          </button>
                        )
                      })
                    )}
                  </div>
                </div>

                {/* Sessions History List */}
                <div className="space-y-2.5 pt-2 border-t border-border/30">
                  <label className="text-xs font-bold uppercase tracking-wider text-muted-foreground flex items-center gap-1.5">
                    <MessageSquare className="w-3.5 h-3.5 text-primary" />
                    <span>Recent Sessions ({sessions.length})</span>
                  </label>
                  <div className="space-y-1.5 max-h-52 overflow-y-auto pr-1">
                    {sessions.length === 0 ? (
                      <p className="text-xs text-muted-foreground/70 py-2 text-center">No active sessions yet.</p>
                    ) : (
                      sessions.map(s => (
                        <div
                          key={s.id}
                          className={cn(
                            'group rounded-xl p-2.5 border transition-all duration-200 flex items-center justify-between gap-2',
                            session?.id === s.id
                              ? 'bg-primary/10 border-primary/40 shadow-sm'
                              : 'bg-card/30 border-border/30 hover:bg-accent/40',
                          )}
                        >
                          <button onClick={() => open(s)} className="flex-1 text-left min-w-0">
                            <p className="text-xs font-semibold text-foreground truncate">{s.title || '(No prompt query)'}</p>
                            <div className="flex items-center gap-2 mt-1">
                              <span className={cn('text-[9px] font-bold uppercase px-1.5 py-0.5 rounded border', stateTone(s.state))}>
                                {s.state}
                              </span>
                              <span className="text-[10px] font-mono text-muted-foreground truncate">{s.id.slice(0, 8)}</span>
                            </div>
                          </button>
                          <button
                            onClick={() => remove(s.id)}
                            title="Remove session"
                            className="opacity-0 group-hover:opacity-100 transition-opacity p-1.5 rounded-lg hover:bg-destructive/10 text-muted-foreground hover:text-destructive"
                          >
                            <Trash2 className="w-3.5 h-3.5" />
                          </button>
                        </div>
                      ))
                    )}
                  </div>
                </div>
              </div>
            </div>
          )}
        </aside>

        {/* Right Main Session Console (expands to take all remaining width) */}
        <section className="flex-1 flex flex-col min-h-0 min-w-0 glass-panel rounded-2xl border border-border/40 overflow-hidden shadow-sm transition-all duration-300">
          {/* Console Header */}
          <div className="px-6 py-3.5 border-b border-border/40 bg-card/40 flex items-center justify-between gap-4">
            <div className="flex items-center gap-3 min-w-0">
              <button
                onClick={() => setSidebarCollapsed(v => !v)}
                className="p-1.5 rounded-lg border border-border/40 text-muted-foreground hover:text-foreground hover:bg-accent/50 transition-all shrink-0"
                title={sidebarCollapsed ? "Expand artifact sidebar" : "Collapse artifact sidebar (expand chat)"}
              >
                {sidebarCollapsed ? <PanelLeftOpen className="w-4 h-4 text-primary" /> : <PanelLeftClose className="w-4 h-4" />}
              </button>
              <div className={cn('w-2.5 h-2.5 rounded-full shrink-0', busy ? 'bg-primary animate-ping' : 'bg-emerald-500')} />
              <div className="min-w-0">
                <h2 className="text-sm font-semibold text-foreground truncate">
                  {session ? session.title || 'Live Session Stream' : 'New Live Search Session'}
                </h2>
                {session && (
                  <p className="text-[11px] text-muted-foreground font-mono truncate">Session ID: {session.id}</p>
                )}
              </div>
            </div>
            {session && (
              <div className="flex items-center gap-2">
                <span className={cn('text-[10px] font-bold uppercase px-2.5 py-1 rounded-lg border', stateTone(session.state))}>
                  {session.state}
                </span>
              </div>
            )}
          </div>

          {/* Warning / Reconnection Banners */}
          {streamQuiet && (
            <div className="px-6 py-2 bg-amber-500/10 border-b border-amber-500/30 text-amber-600 dark:text-amber-400 text-xs flex items-center gap-2">
              <Clock className="w-4 h-4 shrink-0" />
              <span>The connection paused. The agent run continues on the server; reconnecting automatically...</span>
            </div>
          )}
          {problem && (
            <div className="px-6 py-2 bg-destructive/10 border-b border-destructive/30 text-destructive text-xs flex items-center gap-2">
              <AlertCircle className="w-4 h-4 shrink-0" />
              <span>{problem}</span>
            </div>
          )}

          {/* Transcript Scroll Container */}
          <div className="flex-1 overflow-y-auto px-6 py-6 space-y-6">
            {!session ? (
              <div className="h-full flex items-center justify-center py-12">
                <EmptyState
                  icon={Sparkles}
                  title="Start a Live Search Run"
                  description="Select artifacts from the registry on the left, then enter your prompt below to launch an autonomous agent session."
                />
              </div>
            ) : turns.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-16 text-center space-y-3">
                <LoadingSpinner size="md" />
                <p className="text-sm text-muted-foreground">Initializing throwaway project and agent environment...</p>
              </div>
            ) : (
              <div className="space-y-6 max-w-5xl mx-auto">
                {turns.map((t, i) => (
                  <article key={i} className="space-y-3 animate-in fade-in duration-200">
                    {/* User Question */}
                    {t.question && (
                      <div className="flex items-start gap-3 bg-primary/5 border border-primary/15 rounded-2xl p-4 shadow-sm">
                        <div className="w-7 h-7 rounded-xl bg-primary/10 border border-primary/20 flex items-center justify-center shrink-0 text-primary mt-0.5">
                          <User className="w-4 h-4" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <span className="text-[10px] font-bold uppercase tracking-wider text-primary">User Request</span>
                          <p className="text-sm font-semibold text-foreground mt-0.5 leading-relaxed">{t.question}</p>
                        </div>
                      </div>
                    )}

                    {/* Execution / Tool Activity Stream */}
                    {t.activity.length > 0 && (
                      <div className="border-l-2 border-primary/30 pl-4 ml-3 py-1 space-y-2">
                        {t.activity.map(a => (
                          <div key={a.seq} className="flex items-start gap-2 text-xs text-muted-foreground">
                            <Wrench className="w-3.5 h-3.5 text-primary/70 mt-0.5 shrink-0" />
                            <span className="font-mono font-semibold bg-accent/60 px-2 py-0.5 rounded-md border border-border/40 text-foreground text-[11px]">
                              {a.label}
                            </span>
                            {a.detail && (
                              <span className="font-mono text-[11px] text-muted-foreground/80 truncate">{a.detail}</span>
                            )}
                          </div>
                        ))}
                      </div>
                    )}

                    {/* Agent Response Answer */}
                    {t.answer && (
                      <div className="flex items-start gap-3 bg-card/70 border border-border/40 rounded-2xl p-5 shadow-sm">
                        <div className="w-7 h-7 rounded-xl bg-purple-500/10 border border-purple-500/20 flex items-center justify-center shrink-0 text-purple-500 mt-0.5">
                          <Bot className="w-4 h-4" />
                        </div>
                        <div className="flex-1 min-w-0">
                          <span className="text-[10px] font-bold uppercase tracking-wider text-purple-600 dark:text-purple-400">Agent Output</span>
                          <div className="mt-1 text-sm leading-relaxed text-foreground whitespace-pre-wrap font-sans">
                            {t.answer}
                            {!t.done && busy && (
                              <span className="inline-block w-2 h-4 bg-primary rounded-sm animate-pulse ml-1 align-middle" />
                            )}
                          </div>
                        </div>
                      </div>
                    )}

                    {/* Turn Errors */}
                    {t.errors.map((err, j) => (
                      <div key={j} className="flex items-center gap-2 text-xs text-destructive bg-destructive/10 border border-destructive/20 rounded-xl p-3">
                        <AlertCircle className="w-4 h-4 shrink-0" />
                        <span>{err}</span>
                      </div>
                    ))}
                  </article>
                ))}
                <div ref={transcriptEnd} />
              </div>
            )}
          </div>

          {/* Footer Input Area */}
          <footer className="border-t border-border/40 p-4 bg-card/30">
            <div className="flex gap-3 max-w-5xl mx-auto items-end">
              <div className="flex-1 relative">
                <textarea
                  value={question}
                  onChange={e => setQuestion(e.target.value)}
                  onKeyDown={e => {
                    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
                      e.preventDefault()
                      if (session) { if (canAsk) void ask() } else { void start() }
                    }
                  }}
                  rows={2}
                  placeholder={session ? 'Ask a follow-up query…' : 'Enter prompt or question for the live search agent…'}
                  className="w-full text-sm rounded-xl bg-background/60 border border-border/50 backdrop-blur-sm px-3.5 py-2.5 resize-none outline-none focus:ring-2 focus:ring-primary/20 focus:border-primary transition-all shadow-sm"
                />
              </div>

              {session ? (
                <button
                  onClick={ask}
                  disabled={!canAsk}
                  className="btn-premium flex items-center gap-2 text-sm font-semibold px-5 py-3 rounded-xl disabled:opacity-40 shrink-0"
                >
                  <Send className="w-4 h-4" /> Ask
                </button>
              ) : (
                <button
                  onClick={start}
                  disabled={starting || chosen.length === 0 || !activeIde}
                  title={chosen.length === 0 ? 'Choose at least one artifact to search' : undefined}
                  className="btn-premium flex items-center gap-2 text-sm font-semibold px-5 py-3 rounded-xl disabled:opacity-40 shrink-0"
                >
                  {starting ? <LoadingSpinner size="sm" /> : <Radio className="w-4 h-4" />} Start Run
                </button>
              )}
            </div>
            <div className="flex items-center justify-between max-w-5xl mx-auto mt-2 text-[11px] text-muted-foreground/70">
              <span>{session ? 'Press Ctrl+Enter to send follow-up' : 'Choose artifacts on the left & press Ctrl+Enter to launch search'}</span>
              {chosen.length > 0 && !session && (
                <span className="text-primary font-medium">{chosen.length} artifact(s) selected</span>
              )}
            </div>
          </footer>
        </section>
      </div>
    </div>
  )
}


