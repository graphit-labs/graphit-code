import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  ArrowLeft, Check, CheckCircle2, ChevronDown, Clock3, GitBranch, RefreshCw, Search, Users,
} from 'lucide-react'

import { sessionApi, type SessionDetail, type SessionSearchResult, type SessionSpec } from '@/api/taskSession'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { showToast } from '@/hooks/useToast'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

import { TaskModeToggle } from './TaskModeToggle'

const statusStyle: Record<string, string> = {
  open: 'bg-blue-500/10 text-blue-600 dark:text-blue-300 border-blue-500/20',
  in_progress: 'bg-amber-500/10 text-amber-700 dark:text-amber-300 border-amber-500/20',
  completed: 'bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border-emerald-500/20',
  cancelled: 'bg-rose-500/10 text-rose-700 dark:text-rose-300 border-rose-500/20',
}

const statusLabel = (status: string) => status.replace('_', ' ')

function StatusBadge({ status }: { status: string }) {
  return (
    <span className={cn('inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider', statusStyle[status] ?? 'bg-accent text-muted-foreground border-border')}>
      {statusLabel(status)}
    </span>
  )
}

function shortDate(value?: string) {
  if (!value) return '—'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}

const statusOptions = [
  ['all', 'All statuses'],
  ['open', 'Open'],
  ['in_progress', 'In progress'],
  ['completed', 'Completed'],
  ['cancelled', 'Cancelled'],
] as const

function StatusSelector({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const close = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', close)
    return () => document.removeEventListener('mousedown', close)
  }, [])
  const label = statusOptions.find(([id]) => id === value)?.[1] ?? 'All statuses'

  return (
    <div ref={ref} className="relative min-w-0 flex-1">
      <button
        type="button"
        aria-label="Filter session status"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen(current => !current)}
        className="flex w-full items-center gap-2 rounded-xl border border-border/40 bg-background/65 px-3 py-2 text-xs font-semibold text-foreground transition-all hover:bg-accent/45"
      >
        <span className="flex-1 text-left">{label}</span>
        <ChevronDown className={cn('h-3.5 w-3.5 text-muted-foreground transition-transform', open && 'rotate-180')} />
      </button>
      {open && (
        <div role="listbox" aria-label="Session statuses" className="absolute left-0 right-0 top-full z-50 mt-1.5 overflow-hidden rounded-xl border border-border/50 bg-card shadow-2xl animate-in fade-in slide-in-from-top-2 duration-150">
          {statusOptions.map(([id, optionLabel]) => (
            <button
              key={id}
              type="button"
              role="option"
              aria-selected={id === value}
              onClick={() => { onChange(id); setOpen(false) }}
              className={cn('flex w-full items-center gap-3 px-3.5 py-2.5 text-left text-sm font-medium text-foreground transition-colors hover:bg-accent/60', id === value && 'bg-primary/10')}
            >
              {optionLabel}
              {id === value && <Check className="ml-auto h-3.5 w-3.5 shrink-0 text-primary" />}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}

function SessionMarkdown({ content }: { content: string }) {
  return <div className="min-w-0 [&_.wiki-prose>*:last-child]:mb-0"><MarkdownContent content={content} /></div>
}

function SpecSnapshot({ label, spec }: { label: string; spec: SessionSpec }) {
  return (
    <div className="rounded-xl border border-border/35 bg-background/45 p-4">
      <p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">{label}</p>
      {spec.title && <p className="mt-2 text-sm font-bold text-foreground">{spec.title}</p>}
      {spec.description
        ? <div className="mt-3"><SessionMarkdown content={spec.description} /></div>
        : <p className="mt-2 text-sm text-muted-foreground">No description.</p>}
      {spec.strategy && <div className="mt-3 border-t border-border/30 pt-3"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Strategy</p><SessionMarkdown content={spec.strategy} /></div>}
    </div>
  )
}

function SessionDetailView({ detail, onBack, onOpenTask }: { detail: SessionDetail; onBack: () => void; onOpenTask: (id: string) => void }) {
  const session = detail.session
  const checkpoints = [...detail.checkpoints].sort((a, b) => a.sequence - b.sequence)
  const revisions = [...detail.spec_revisions].sort((a, b) => a.source_revision - b.source_revision)

  return (
    <div className="h-full overflow-y-auto">
      <header className="sticky top-0 z-10 border-b border-border/40 bg-background/90 px-6 py-4 backdrop-blur-xl">
        <button type="button" onClick={onBack} className="mb-3 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground md:hidden"><ArrowLeft className="h-3.5 w-3.5" /> Sessions</button>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <StatusBadge status={session.status} />
              {session.owner && <span className="inline-flex items-center gap-1 rounded-full border border-border/50 bg-accent/30 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-muted-foreground"><Users className="h-3 w-3" /> {session.owner}</span>}
            </div>
            <h1 className="text-2xl font-black tracking-tight text-foreground">{session.title}</h1>
            <p className="mt-1 font-mono text-xs text-muted-foreground">{session.id} · rev {session.revision}</p>
          </div>
        </div>
      </header>

      <div className="mx-auto max-w-6xl space-y-5 p-6">
        <section className="glass-panel rounded-2xl p-5">
          <p className="mb-2 text-[10px] font-bold uppercase tracking-[0.18em] text-muted-foreground">Description</p>
          <SessionMarkdown content={session.description} />
        </section>

        {session.strategy && (
          <section className="glass-panel rounded-2xl p-5">
            <p className="mb-2 text-[10px] font-bold uppercase tracking-[0.18em] text-muted-foreground">Current strategy</p>
            <SessionMarkdown content={session.strategy} />
          </section>
        )}

        {(session.progress_summary || session.next_step) && (
          <section className="grid gap-3 md:grid-cols-2">
            {session.progress_summary && <div className="glass-panel rounded-2xl p-4"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Latest progress</p><SessionMarkdown content={session.progress_summary} /></div>}
            {session.next_step && <div className="glass-panel rounded-2xl p-4"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Next step</p><SessionMarkdown content={session.next_step} /></div>}
          </section>
        )}

        <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {[
            ['Owner', session.owner || 'Unclaimed'],
            ['Created', shortDate(session.created_at)],
            ['Updated', shortDate(session.updated_at)],
            ['Lease', shortDate(session.lease_expires_at)],
          ].map(([label, value]) => <div key={label} className="rounded-2xl border border-border/40 bg-card/45 p-4"><p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">{label}</p><p className="mt-1 truncate text-xs font-semibold text-foreground" title={value}>{value}</p></div>)}
        </section>

        <section className="glass-panel rounded-2xl p-5">
          <div className="mb-4 flex items-center gap-2"><GitBranch className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Linked tasks</h2><span className="ml-auto text-xs text-muted-foreground">{detail.tasks.length}</span></div>
          <div className="space-y-2">
            {detail.tasks.map(task => (
              <button key={task.id} type="button" onClick={() => onOpenTask(task.id)} className="flex w-full items-center justify-between gap-3 rounded-xl border border-border/35 bg-background/45 p-3 text-left transition-colors hover:border-primary/35 hover:bg-primary/5">
                <span className="min-w-0 truncate text-sm font-bold text-foreground">{task.title}</span>
                <span className="flex shrink-0 items-center gap-2"><StatusBadge status={task.status} /><span className="font-mono text-[10px] text-muted-foreground">{task.id}</span></span>
              </button>
            ))}
            {detail.tasks.length === 0 && <p className="text-sm text-muted-foreground">No tasks are linked to this session yet.</p>}
          </div>
        </section>

        <section className="glass-panel rounded-2xl p-5"><div className="mb-4 flex items-center gap-2"><CheckCircle2 className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Checkpoints</h2><span className="ml-auto text-xs text-muted-foreground">{checkpoints.length}</span></div><div className="space-y-3">{checkpoints.map(checkpoint => <div key={checkpoint.key} className="grid gap-1 border-l border-border pl-4 sm:grid-cols-[170px_1fr]"><p className="font-mono text-[10px] text-muted-foreground">{shortDate(checkpoint.at)}<br />rev {checkpoint.revision}</p><div><SessionMarkdown content={checkpoint.summary} />{checkpoint.problems && <div className="mt-2"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Problems</p><SessionMarkdown content={checkpoint.problems} /></div>}{checkpoint.decisions && <div className="mt-2"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Decisions</p><SessionMarkdown content={checkpoint.decisions} /></div>}{checkpoint.next_step && <div className="mt-2"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Next step</p><SessionMarkdown content={checkpoint.next_step} /></div>}<p className="mt-1 text-[10px] text-muted-foreground">{checkpoint.actor}</p></div></div>)}{checkpoints.length === 0 && <p className="text-sm text-muted-foreground">No checkpoints recorded.</p>}</div></section>

        <section className="glass-panel rounded-2xl p-5"><div className="mb-4 flex items-center gap-2"><Clock3 className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Specification revisions</h2><span className="ml-auto text-xs text-muted-foreground">{revisions.length}</span></div><div className="space-y-3">{revisions.map(revision => <details key={revision.key} className="rounded-xl border border-border/35 bg-background/45 p-3"><summary className="cursor-pointer text-sm font-bold">rev {revision.source_revision}</summary><div className="mt-2"><SessionMarkdown content={revision.reason} /></div><p className="mt-2 text-[10px] text-muted-foreground">{revision.actor} · {shortDate(revision.at)}</p><div className="mt-3 grid gap-3 xl:grid-cols-2"><SpecSnapshot label="Before" spec={revision.before} /><SpecSnapshot label="After" spec={revision.after} /></div></details>)}{revisions.length === 0 && <p className="text-sm text-muted-foreground">No specification revisions recorded.</p>}</div></section>
      </div>
    </div>
  )
}

export default function SessionExplorerPage() {
  const { sessionId } = useParams<{ sessionId: string }>()
  const navigate = useNavigate()
  const { activeProjectDir, projectName } = useAppStore()
  const [catalog, setCatalog] = useState<SessionSearchResult[]>([])
  const [nextCursor, setNextCursor] = useState('')
  const [detailResult, setDetailResult] = useState<{
    projectDir: string
    selectedID: string
    detail: SessionDetail
  } | null>(null)
  const [selectedID, setSelectedID] = useState(sessionId ? decodeURIComponent(sessionId) : '')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [activeOnly, setActiveOnly] = useState(false)
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const selectedIDRef = useRef(selectedID)
  const catalogRequestRef = useRef(0)
  const detailRequestRef = useRef(0)
  const previousProjectRef = useRef(activeProjectDir)
  const detail = detailResult?.projectDir === activeProjectDir && detailResult.selectedID === selectedID
    ? detailResult.detail
    : null

  const loadCatalog = useCallback(async (cursor = '', append = false) => {
    const request = ++catalogRequestRef.current
    if (append) setLoadingMore(true)
    else {
      setLoading(true)
      setCatalog([])
      setNextCursor('')
    }
    try {
      const page = await sessionApi.list({
        projectDir: activeProjectDir || undefined,
        query: query.trim() || undefined,
        status,
        active: activeOnly,
        pageSize: 20,
        cursor: cursor || undefined,
      })
      if (request !== catalogRequestRef.current) return
      setCatalog(current => append
        ? [...new Map([...current, ...page.results].map(session => [session.id, session])).values()]
        : page.results)
      setNextCursor(page.next_cursor)
      if (!append && !selectedIDRef.current && page.results[0]) {
        setSelectedID(page.results[0].id)
        selectedIDRef.current = page.results[0].id
      }
      window.document.title = `Graphit Sessions — ${projectName || 'Explorer'}`
    } catch {
      if (request !== catalogRequestRef.current) return
      showToast('Failed to load sessions', 'error')
      if (!append) {
        setCatalog([])
        setNextCursor('')
      }
    } finally {
      if (request === catalogRequestRef.current) {
        setLoading(false)
        setLoadingMore(false)
      }
    }
  }, [activeProjectDir, projectName, query, status, activeOnly])

  useEffect(() => {
    const timer = window.setTimeout(() => { void loadCatalog() }, 200)
    return () => window.clearTimeout(timer)
  }, [loadCatalog])

  useEffect(() => {
    if (previousProjectRef.current === activeProjectDir) return
    previousProjectRef.current = activeProjectDir
    selectedIDRef.current = ''
    setSelectedID('')
    setDetailResult(null)
    navigate('/task/sessions', { replace: true })
  }, [activeProjectDir, navigate])

  useEffect(() => {
    const request = ++detailRequestRef.current
    if (!selectedID) return
    sessionApi.get(activeProjectDir || undefined, selectedID)
      .then(detail => {
        if (request === detailRequestRef.current) {
          setDetailResult({ projectDir: activeProjectDir, selectedID, detail })
        }
      })
      .catch(() => { if (request === detailRequestRef.current) showToast('Failed to load session details', 'error') })
  }, [activeProjectDir, selectedID])

  const selectSession = (id: string) => {
    setDetailResult(null)
    setSelectedID(id)
    selectedIDRef.current = id
    navigate(`/task/sessions/${encodeURIComponent(id)}`, { replace: true })
  }
  const showSessionList = () => {
    setSelectedID('')
    selectedIDRef.current = ''
    setDetailResult(null)
    navigate('/task/sessions', { replace: true })
  }
  const openTask = (id: string) => navigate(`/task/explorer/${encodeURIComponent(id)}`)

  return (
    <div className="explorer-frame flex h-screen overflow-hidden bg-background text-foreground">
      <aside className={cn('w-full shrink-0 flex-col border-r border-border/40 bg-card/45 backdrop-blur-2xl md:flex md:w-[340px]', detail ? 'hidden' : 'flex')}>
        <div className="border-b border-border/40 p-4">
          <button type="button" onClick={() => navigate('/hub/registry')} className="mb-4 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground"><ArrowLeft className="h-3.5 w-3.5" /> Observatory</button>
          <div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-bold uppercase tracking-[0.18em] text-primary">Task / sessions</p><h1 className="mt-1 text-xl font-black tracking-tight">{projectName || 'Project sessions'}</h1></div><button type="button" onClick={() => void loadCatalog()} title="Refresh sessions" className="rounded-xl border border-border/40 bg-background/50 p-2 text-muted-foreground hover:text-foreground"><RefreshCw className="h-4 w-4" /></button></div>
          <TaskModeToggle mode="sessions" />
          <div className="relative mt-4"><Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" /><input aria-label="Search sessions" value={query} onChange={event => setQuery(event.target.value)} placeholder="Search request, strategy…" className="w-full rounded-xl border border-border/40 bg-background/65 py-2.5 pl-9 pr-3 text-sm outline-none focus:border-primary/50" /></div>
          <div className="mt-3 flex items-center gap-2">
            <StatusSelector value={status} onChange={setStatus} />
            <button type="button" aria-pressed={activeOnly} onClick={() => setActiveOnly(current => !current)} title="Show only unfinished sessions" className={cn('rounded-xl border px-3 py-2 text-xs font-bold transition-colors', activeOnly ? 'border-primary/35 bg-primary/10 text-primary' : 'border-border/40 bg-background/65 text-muted-foreground hover:text-foreground')}>Active only</button>
          </div>
        </div>
        <div className="flex items-center justify-between px-4 py-2 text-[10px] font-bold uppercase tracking-wider text-muted-foreground"><span>Sessions</span><span>{catalog.length}{nextCursor ? '+' : ''}</span></div>
        <div aria-label="Session catalogue" className="flex-1 space-y-1 overflow-y-auto px-2 pb-3">
          {loading && <div className="flex justify-center py-10"><LoadingSpinner size="sm" /></div>}
          {!loading && catalog.map(session => <button key={session.id} type="button" onClick={() => selectSession(session.id)} className={cn('w-full rounded-xl border px-3 py-3 text-left transition-colors', selectedID === session.id ? 'border-primary/35 bg-primary/10' : 'border-transparent hover:border-border/40 hover:bg-accent/35')}><p className="line-clamp-2 text-sm font-bold text-foreground">{session.title}</p><div className="mt-2 flex items-center gap-2"><StatusBadge status={session.status} /><span className="font-mono text-[10px] text-muted-foreground">{session.id}</span><span className="ml-auto text-[10px] text-muted-foreground">{shortDate(session.updated_at)}</span></div></button>)}
          {!loading && nextCursor && <button type="button" onClick={() => void loadCatalog(nextCursor, true)} disabled={loadingMore} className="mt-2 w-full rounded-xl border border-border/40 bg-background/45 px-3 py-2.5 text-xs font-bold text-muted-foreground transition-colors hover:bg-accent/45 hover:text-foreground disabled:cursor-wait disabled:opacity-50">{loadingMore ? 'Loading…' : 'Load more'}</button>}
          {!loading && catalog.length === 0 && <div className="px-4 py-10 text-center text-sm text-muted-foreground">No sessions match this view.</div>}
        </div>
      </aside>
      <main className={cn('min-w-0 flex-1 bg-background/95 md:block', detail ? 'block' : 'hidden')}>
        {detail ? <SessionDetailView detail={detail} onBack={showSessionList} onOpenTask={openTask} /> : loading ? <div className="flex h-full items-center justify-center"><LoadingSpinner size="md" /></div> : <EmptyState icon={GitBranch} title="Select a session" description="Choose a session to inspect its evolving request and linked tasks." />}
      </main>
    </div>
  )
}
