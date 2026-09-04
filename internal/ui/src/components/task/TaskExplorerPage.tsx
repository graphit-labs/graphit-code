import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  AlertTriangle, ArrowLeft, Check, CheckCircle2, ChevronDown, Circle, Clock3, Download,
  Flag, GitBranch, MessageSquareText, RefreshCw, Search, ShieldCheck,
  XCircle,
} from 'lucide-react'

import { taskApi, type TaskCatalogItem, type TaskExportDocument } from '@/api/task'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { showToast } from '@/hooks/useToast'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'

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
  ['blocked', 'Blocked'],
  ['flagged', 'Flagged'],
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
        aria-label="Filter task status"
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen(current => !current)}
        className="flex w-full items-center gap-2 rounded-xl border border-border/40 bg-background/65 px-3 py-2 text-xs font-semibold text-foreground transition-all hover:bg-accent/45"
      >
        <span className="flex-1 text-left">{label}</span>
        <ChevronDown className={cn('h-3.5 w-3.5 text-muted-foreground transition-transform', open && 'rotate-180')} />
      </button>
      {open && (
        <div role="listbox" aria-label="Task statuses" className="absolute left-0 right-0 top-full z-50 mt-1.5 overflow-hidden rounded-xl border border-border/50 bg-card shadow-2xl animate-in fade-in slide-in-from-top-2 duration-150">
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

function downloadJSON(document: TaskExportDocument, filename: string) {
  const blob = new Blob([JSON.stringify(document, null, 2) + '\n'], { type: 'application/json' })
  const url = URL.createObjectURL(blob)
  const anchor = window.document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  URL.revokeObjectURL(url)
}

function TaskDetail({ document, onBack }: { document: TaskExportDocument; onBack: () => void }) {
  const task = document.tasks.find(item => item.id === document.task_id) ?? document.tasks[0]
  if (!task) return <EmptyState icon={AlertTriangle} title="Task not found" description="The exported task document is empty." />

  const subtasks = document.tasks.filter(item => item.parent_id === task.id)
  const dependencies = document.dependencies.filter(item => item.task_id === task.id)
  const checks = document.checks.filter(item => item.task_id === task.id)
  const events = document.events.filter(item => item.task_id === task.id)
  const comments = document.comments.filter(item => item.task_id === task.id)
  const revisions = document.spec_revisions.filter(item => item.task_id === task.id)

  return (
    <div className="h-full overflow-y-auto">
      <header className="sticky top-0 z-10 border-b border-border/40 bg-background/90 px-6 py-4 backdrop-blur-xl">
        <button type="button" onClick={onBack} className="mb-3 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground md:hidden"><ArrowLeft className="h-3.5 w-3.5" /> Tasks</button>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0">
            <div className="mb-2 flex flex-wrap items-center gap-2">
              <StatusBadge status={task.status} />
              <span className="rounded-full border border-border/50 bg-accent/30 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">P{task.priority}</span>
              {task.flagged && <span className="inline-flex items-center gap-1 rounded-full border border-rose-500/25 bg-rose-500/10 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wider text-rose-600 dark:text-rose-300"><Flag className="h-3 w-3" /> Flagged</span>}
            </div>
            <h1 className="text-2xl font-black tracking-tight text-foreground">{task.title}</h1>
            <p className="mt-1 font-mono text-xs text-muted-foreground">{task.id} · rev {task.revision} · {task.type}</p>
          </div>
          <button
            type="button"
            onClick={() => downloadJSON(document, `${task.id}.json`)}
            className="inline-flex items-center gap-2 rounded-xl border border-border/50 bg-card px-3 py-2 text-xs font-bold text-foreground transition-colors hover:bg-accent"
          >
            <Download className="h-3.5 w-3.5" /> Export JSON
          </button>
        </div>
      </header>

      <div className="mx-auto max-w-6xl space-y-5 p-6">
        <section className="glass-panel rounded-2xl p-5">
          <p className="mb-2 text-[10px] font-bold uppercase tracking-[0.18em] text-muted-foreground">Specification</p>
          <p className="whitespace-pre-wrap text-sm leading-6 text-foreground/85">{task.description}</p>
        </section>

        {(task.progress_summary || task.next_step || task.flag_reason) && (
          <section className="grid gap-3 md:grid-cols-2">
            {task.progress_summary && <div className="glass-panel rounded-2xl p-4"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Latest progress</p><p className="text-sm text-foreground/85">{task.progress_summary}</p></div>}
            {task.next_step && <div className="glass-panel rounded-2xl p-4"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Next step</p><p className="text-sm text-foreground/85">{task.next_step}</p></div>}
            {task.flag_reason && <div className="glass-panel rounded-2xl border-rose-500/20 p-4 md:col-span-2"><p className="mb-1 text-[10px] font-bold uppercase tracking-wider text-rose-600 dark:text-rose-300">Flag reason</p><p className="text-sm text-foreground/85">{task.flag_reason}</p></div>}
          </section>
        )}

        <section className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {[
            ['Owner', task.owner || 'Unclaimed'],
            ['Created', shortDate(task.created_at)],
            ['Updated', shortDate(task.updated_at)],
            ['Lease', shortDate(task.lease_expires_at)],
          ].map(([label, value]) => <div key={label} className="rounded-2xl border border-border/40 bg-card/45 p-4"><p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">{label}</p><p className="mt-1 truncate text-xs font-semibold text-foreground" title={value}>{value}</p></div>)}
        </section>

        <section className="grid gap-5 xl:grid-cols-2">
          <div className="glass-panel rounded-2xl p-5">
            <div className="mb-4 flex items-center justify-between"><h2 className="flex items-center gap-2 text-sm font-black"><ShieldCheck className="h-4 w-4 text-primary" /> Checks</h2><span className="text-xs text-muted-foreground">{checks.length}</span></div>
            <div className="space-y-2">
              {checks.map(check => <div key={check.key} className="rounded-xl border border-border/35 bg-background/45 p-3"><div className="flex items-start gap-2">{check.status === 'passed' ? <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-emerald-500" /> : check.status === 'failed' ? <XCircle className="mt-0.5 h-4 w-4 shrink-0 text-rose-500" /> : <Circle className="mt-0.5 h-4 w-4 shrink-0 text-muted-foreground" />}<div><p className="text-xs font-bold uppercase tracking-wider text-muted-foreground">{check.kind} · {check.status}</p><p className="mt-1 text-sm text-foreground/85">{check.text}</p>{check.evidence && <p className="mt-2 text-xs text-muted-foreground">{check.evidence}</p>}{check.superseded_reason && <p className="mt-2 text-xs text-amber-600 dark:text-amber-300">{check.superseded_reason}</p>}</div></div></div>)}
              {checks.length === 0 && <p className="text-sm text-muted-foreground">No checks recorded.</p>}
            </div>
          </div>

          <div className="space-y-5">
            <div className="glass-panel rounded-2xl p-5"><div className="mb-3 flex items-center gap-2"><GitBranch className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Relations</h2></div><div className="space-y-2 text-sm"><p><span className="text-muted-foreground">Parent:</span> <span className="font-mono">{task.parent_id || '—'}</span></p><p><span className="text-muted-foreground">Dependencies:</span> {dependencies.filter(item => item.active).map(item => item.depends_on).join(', ') || '—'}</p><p><span className="text-muted-foreground">Subtasks:</span> {subtasks.map(item => item.id).join(', ') || '—'}</p><p><span className="text-muted-foreground">Blocked by:</span> {task.blocked_by?.join(', ') || '—'}</p></div></div>
            <div className="glass-panel rounded-2xl p-5"><div className="mb-3 flex items-center gap-2"><MessageSquareText className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Comments</h2><span className="ml-auto text-xs text-muted-foreground">{comments.length}</span></div><div className="space-y-3">{comments.map(comment => <div key={comment.id} className="border-l-2 border-primary/35 pl-3"><p className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">{comment.kind} · {comment.actor} · {shortDate(comment.at)}</p><p className="mt-1 text-sm text-foreground/85">{comment.body}</p></div>)}{comments.length === 0 && <p className="text-sm text-muted-foreground">No comments recorded.</p>}</div></div>
          </div>
        </section>

        <section className="glass-panel rounded-2xl p-5"><div className="mb-4 flex items-center gap-2"><Clock3 className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Lifecycle</h2><span className="ml-auto text-xs text-muted-foreground">{events.length} events</span></div><div className="space-y-3">{events.map(event => <div key={event.key} className="grid gap-1 border-l border-border pl-4 sm:grid-cols-[170px_1fr]"><p className="font-mono text-[10px] text-muted-foreground">{shortDate(event.at)}<br />rev {event.revision}</p><div><p className="text-xs font-bold uppercase tracking-wider text-foreground">{statusLabel(event.type)}</p>{event.summary && <p className="mt-1 text-sm text-foreground/75">{event.summary}</p>}<p className="mt-1 text-[10px] text-muted-foreground">{event.actor}</p></div></div>)}</div></section>

        <section className="glass-panel rounded-2xl p-5"><div className="mb-4 flex items-center gap-2"><RefreshCw className="h-4 w-4 text-primary" /><h2 className="text-sm font-black">Specification revisions</h2><span className="ml-auto text-xs text-muted-foreground">{revisions.length}</span></div><div className="space-y-3">{revisions.map(revision => <details key={revision.key} className="rounded-xl border border-border/35 bg-background/45 p-3"><summary className="cursor-pointer text-sm font-bold">rev {revision.source_revision} · {statusLabel(revision.kind)}</summary><p className="mt-2 text-sm text-foreground/80">{revision.reason}</p><p className="mt-2 text-[10px] text-muted-foreground">{revision.actor} · {shortDate(revision.at)}</p><pre className="mt-3 max-h-72 overflow-auto rounded-lg bg-black/85 p-3 text-[10px] leading-5 text-white/75">{JSON.stringify({ before: revision.before, after: revision.after }, null, 2)}</pre></details>)}{revisions.length === 0 && <p className="text-sm text-muted-foreground">No specification revisions recorded.</p>}</div></section>

        <details className="glass-panel rounded-2xl p-5"><summary className="cursor-pointer text-sm font-black">Complete JSON document</summary><pre className="mt-4 max-h-[520px] overflow-auto rounded-xl bg-black/90 p-4 text-[11px] leading-5 text-white/80">{JSON.stringify(document, null, 2)}</pre></details>
      </div>
    </div>
  )
}

export default function TaskExplorerPage() {
  const { taskId } = useParams<{ taskId: string }>()
  const navigate = useNavigate()
  const { activeProjectDir, projectName } = useAppStore()
  const [catalog, setCatalog] = useState<TaskCatalogItem[]>([])
  const [nextCursor, setNextCursor] = useState('')
  const [detail, setDetail] = useState<TaskExportDocument | null>(null)
  const [selectedID, setSelectedID] = useState(taskId ? decodeURIComponent(taskId) : '')
  const [query, setQuery] = useState('')
  const [status, setStatus] = useState('all')
  const [loading, setLoading] = useState(true)
  const [loadingMore, setLoadingMore] = useState(false)
  const [exporting, setExporting] = useState(false)
  const selectedIDRef = useRef(selectedID)
  const catalogRequestRef = useRef(0)
  const detailRequestRef = useRef(0)
  const previousProjectRef = useRef(activeProjectDir)

  const loadCatalog = useCallback(async (cursor = '', append = false) => {
    const request = ++catalogRequestRef.current
    if (append) setLoadingMore(true)
    else {
      setLoading(true)
      setCatalog([])
      setNextCursor('')
    }
    try {
      const page = await taskApi.list({
        projectDir: activeProjectDir || undefined,
        query: query.trim() || undefined,
        status,
        pageSize: 20,
        cursor: cursor || undefined,
      })
      if (request !== catalogRequestRef.current) return
      setCatalog(current => append
        ? [...new Map([...current, ...page.results].map(task => [task.id, task])).values()]
        : page.results)
      setNextCursor(page.next_cursor)
      if (!append && !selectedIDRef.current && page.results[0]) {
        setSelectedID(page.results[0].id)
        selectedIDRef.current = page.results[0].id
      }
      window.document.title = `Graphit Tasks — ${projectName || 'Explorer'}`
    } catch {
      if (request !== catalogRequestRef.current) return
      showToast('Failed to load tasks', 'error')
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
  }, [activeProjectDir, projectName, query, status])

  useEffect(() => {
    const timer = window.setTimeout(() => { void loadCatalog() }, 200)
    return () => window.clearTimeout(timer)
  }, [loadCatalog])

  useEffect(() => {
    if (previousProjectRef.current === activeProjectDir) return
    previousProjectRef.current = activeProjectDir
    selectedIDRef.current = ''
    setSelectedID('')
    setDetail(null)
    navigate('/task/explorer', { replace: true })
  }, [activeProjectDir, navigate])

  useEffect(() => {
    const request = ++detailRequestRef.current
    if (!selectedID) { setDetail(null); return }
    setDetail(null)
    taskApi.export(activeProjectDir || undefined, selectedID)
      .then(document => { if (request === detailRequestRef.current) setDetail(document) })
      .catch(() => { if (request === detailRequestRef.current) { setDetail(null); showToast('Failed to load task details', 'error') } })
  }, [activeProjectDir, selectedID])

  const selectTask = (id: string) => {
    setSelectedID(id)
    selectedIDRef.current = id
    navigate(`/task/explorer/${encodeURIComponent(id)}`, { replace: true })
  }
  const showTaskList = () => {
    setSelectedID('')
    selectedIDRef.current = ''
    setDetail(null)
    navigate('/task/explorer', { replace: true })
  }
  const exportAll = async () => {
    setExporting(true)
    try {
      const document = await taskApi.export(activeProjectDir || undefined)
      downloadJSON(document, 'graphit-tasks.json')
    } catch {
      showToast('Failed to export tasks', 'error')
    } finally {
      setExporting(false)
    }
  }

  return (
    <div className="explorer-frame flex h-screen overflow-hidden bg-background text-foreground">
      <aside className={cn('w-full shrink-0 flex-col border-r border-border/40 bg-card/45 backdrop-blur-2xl md:flex md:w-[340px]', detail ? 'hidden' : 'flex')}>
        <div className="border-b border-border/40 p-4">
          <button type="button" onClick={() => navigate('/hub/registry')} className="mb-4 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground"><ArrowLeft className="h-3.5 w-3.5" /> Observatory</button>
          <div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-bold uppercase tracking-[0.18em] text-primary">Task / explorer</p><h1 className="mt-1 text-xl font-black tracking-tight">{projectName || 'Project tasks'}</h1></div><button type="button" onClick={() => void loadCatalog()} title="Refresh tasks" className="rounded-xl border border-border/40 bg-background/50 p-2 text-muted-foreground hover:text-foreground"><RefreshCw className="h-4 w-4" /></button></div>
          <div className="relative mt-4"><Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" /><input aria-label="Search tasks" value={query} onChange={event => setQuery(event.target.value)} placeholder="Search ID, title, spec…" className="w-full rounded-xl border border-border/40 bg-background/65 py-2.5 pl-9 pr-3 text-sm outline-none focus:border-primary/50" /></div>
          <div className="mt-3 flex items-center gap-2"><StatusSelector value={status} onChange={setStatus} /><button type="button" aria-label="Export all tasks" onClick={() => void exportAll()} disabled={exporting} title="Export all tasks" className="rounded-xl border border-border/40 bg-background/65 p-2 text-muted-foreground transition-colors hover:text-foreground disabled:cursor-wait disabled:opacity-50"><Download className="h-4 w-4" /></button></div>
        </div>
        <div className="flex items-center justify-between px-4 py-2 text-[10px] font-bold uppercase tracking-wider text-muted-foreground"><span>Tasks</span><span>{catalog.length}{nextCursor ? '+' : ''}</span></div>
        <div aria-label="Task catalogue" className="flex-1 space-y-1 overflow-y-auto px-2 pb-3">
          {loading && <div className="flex justify-center py-10"><LoadingSpinner size="sm" /></div>}
          {!loading && catalog.map(task => <button key={task.id} type="button" onClick={() => selectTask(task.id)} className={cn('w-full rounded-xl border px-3 py-3 text-left transition-colors', selectedID === task.id ? 'border-primary/35 bg-primary/10' : 'border-transparent hover:border-border/40 hover:bg-accent/35')}><div className="flex items-start justify-between gap-2"><p className="line-clamp-2 text-sm font-bold text-foreground">{task.title}</p>{task.flagged ? <Flag className="h-3.5 w-3.5 shrink-0 text-rose-500" /> : task.blocked_by?.length ? <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" /> : null}</div><div className="mt-2 flex items-center gap-2"><StatusBadge status={task.status} /><span className="font-mono text-[10px] text-muted-foreground">{task.id}</span><span className="ml-auto text-[10px] font-bold text-muted-foreground">P{task.priority}</span></div></button>)}
          {!loading && nextCursor && <button type="button" onClick={() => void loadCatalog(nextCursor, true)} disabled={loadingMore} className="mt-2 w-full rounded-xl border border-border/40 bg-background/45 px-3 py-2.5 text-xs font-bold text-muted-foreground transition-colors hover:bg-accent/45 hover:text-foreground disabled:cursor-wait disabled:opacity-50">{loadingMore ? 'Loading…' : 'Load more'}</button>}
          {!loading && catalog.length === 0 && <div className="px-4 py-10 text-center text-sm text-muted-foreground">No tasks match this view.</div>}
        </div>
      </aside>
      <main className={cn('min-w-0 flex-1 bg-background/95 md:block', detail ? 'block' : 'hidden')}>
        {detail ? <TaskDetail document={detail} onBack={showTaskList} /> : loading ? <div className="flex h-full items-center justify-center"><LoadingSpinner size="md" /></div> : <EmptyState icon={ShieldCheck} title="Select a task" description="Choose a task to inspect its complete deterministic record." />}
      </main>
    </div>
  )
}
