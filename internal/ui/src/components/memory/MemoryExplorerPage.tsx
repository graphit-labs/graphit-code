import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import {
  AlertTriangle, ArrowLeft, Brain, CheckCircle2, Edit3, FileClock,
  Fingerprint, Flag, Plus, RefreshCw, Save, Search, ShieldCheck, Tag, Trash2, X,
} from 'lucide-react'

import {
  memoryApi, sortMemoryCatalogItems, type MemoryCatalog, type MemoryCatalogItem, type MemoryScope,
  type MemoryTrace, type MemoryUpdate, type MemoryVersion, type MemoryWrite,
} from '@/api/memory'
import { EmptyState } from '@/components/shared/EmptyState'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { showToast } from '@/hooks/useToast'
import { cn } from '@/lib/utils'
import { useAppStore } from '@/store/appStore'
import { MemoryMarkdown } from './MemoryMarkdown'

const memoryTypes = ['fact', 'decision', 'convention', 'correction', 'tension', 'skill']

const typeStyle: Record<string, string> = {
  fact: 'border-sky-500/20 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  decision: 'border-violet-500/20 bg-violet-500/10 text-violet-700 dark:text-violet-300',
  convention: 'border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  correction: 'border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  tension: 'border-rose-500/20 bg-rose-500/10 text-rose-700 dark:text-rose-300',
  skill: 'border-cyan-500/20 bg-cyan-500/10 text-cyan-700 dark:text-cyan-300',
}

function shortDate(value?: string) {
  if (!value) return '—'
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString()
}

function TypeBadge({ type }: { type: string }) {
  return <span className={cn('rounded-full border px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider', typeStyle[type] ?? 'border-border bg-accent text-muted-foreground')}>{type || 'untyped'}</span>
}

function MemoryFlags({ important, mandatory }: { important: boolean; mandatory: boolean }) {
  return (
    <div className="flex items-center gap-1.5">
      {important && <span title="Important" className="inline-flex items-center gap-1 rounded-full border border-amber-500/20 bg-amber-500/10 px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider text-amber-700 dark:text-amber-300"><Flag className="h-2.5 w-2.5" /> Important</span>}
      {mandatory && <span title="Mandatory at session start" className="inline-flex items-center gap-1 rounded-full border border-emerald-500/20 bg-emerald-500/10 px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider text-emerald-700 dark:text-emerald-300"><ShieldCheck className="h-2.5 w-2.5" /> Mandatory</span>}
    </div>
  )
}

function Meta({ label, value, mono = false }: { label: string; value?: string | number; mono?: boolean }) {
  return (
    <div className="min-w-0 rounded-xl border border-border/35 bg-background/45 p-3">
      <p className="text-[9px] font-bold uppercase tracking-[0.16em] text-muted-foreground">{label}</p>
      <p className={cn('mt-1 break-all text-xs font-semibold text-foreground', mono && 'font-mono text-[11px]')}>{value || '—'}</p>
    </div>
  )
}

interface MemoryFormProps {
  initial?: MemoryVersion
  saving: boolean
  onClose: () => void
  onSave: (value: MemoryWrite | MemoryUpdate) => void
}

function MemoryForm({ initial, saving, onClose, onSave }: MemoryFormProps) {
  const [title, setTitle] = useState(initial?.title ?? '')
  const [body, setBody] = useState(initial?.body ?? '')
  const [type, setType] = useState(initial?.type ?? 'fact')
  const [tags, setTags] = useState(initial?.tags.filter(tag => !['memory', initial.scope, initial.type].includes(tag)).join(', ') ?? '')
  const [important, setImportant] = useState(initial?.important ?? false)
  const [mandatory, setMandatory] = useState(initial?.mandatory ?? false)
  const valid = title.trim() !== '' && body.trim() !== ''

  const submit = (event: React.FormEvent) => {
    event.preventDefault()
    if (!valid) return
    const value: MemoryWrite = {
      title: title.trim(), body: body.trim(), type, important, mandatory,
      ...(!initial ? { tags: tags.split(',').map(tag => tag.trim()).filter(Boolean) } : {}),
    }
    onSave(value)
  }

  return (
    <div role="dialog" aria-modal="true" aria-label={initial ? 'Edit memory' : 'Create memory'} className="fixed inset-0 z-[100] flex items-center justify-center bg-black/60 p-4 backdrop-blur-sm">
      <form onSubmit={submit} className="w-full max-w-2xl overflow-hidden rounded-2xl border border-border/55 bg-card shadow-2xl">
        <div className="flex items-center justify-between border-b border-border/40 px-5 py-4">
          <div><p className="text-[10px] font-bold uppercase tracking-[0.18em] text-primary">Memory / {initial ? 'edit' : 'create'}</p><h2 className="mt-1 text-lg font-black">{initial ? 'Refine this memory' : 'Capture a durable memory'}</h2></div>
          <button type="button" aria-label="Close memory form" onClick={onClose} className="rounded-lg p-2 text-muted-foreground hover:bg-accent hover:text-foreground"><X className="h-4 w-4" /></button>
        </div>
        <div className="max-h-[72vh] space-y-4 overflow-y-auto p-5">
          <label className="block"><span className="mb-1.5 block text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Title</span><input aria-label="Memory title" autoFocus value={title} onChange={event => setTitle(event.target.value)} className="w-full rounded-xl border border-border/45 bg-background/65 px-3 py-2.5 text-sm outline-none focus:border-primary/60" /></label>
          <label className="block"><span className="mb-1.5 block text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Content</span><textarea aria-label="Memory content" value={body} onChange={event => setBody(event.target.value)} rows={10} className="w-full resize-y rounded-xl border border-border/45 bg-background/65 px-3 py-2.5 font-mono text-sm leading-relaxed outline-none focus:border-primary/60" /></label>
          <div className="grid gap-3 sm:grid-cols-2">
            <label><span className="mb-1.5 block text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Type</span><select aria-label="Memory type" value={type} onChange={event => setType(event.target.value)} className="w-full rounded-xl border border-border/45 bg-background/65 px-3 py-2.5 text-sm outline-none focus:border-primary/60">{memoryTypes.map(value => <option key={value} value={value}>{value}</option>)}</select></label>
            {!initial && <label><span className="mb-1.5 block text-[10px] font-bold uppercase tracking-wider text-muted-foreground">Tags</span><input aria-label="Memory tags" value={tags} onChange={event => setTags(event.target.value)} placeholder="architecture, storage" className="w-full rounded-xl border border-border/45 bg-background/65 px-3 py-2.5 text-sm outline-none focus:border-primary/60" /></label>}
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="flex cursor-pointer items-start gap-3 rounded-xl border border-border/35 bg-background/45 p-3"><input aria-label="Important memory" type="checkbox" checked={important} onChange={event => setImportant(event.target.checked)} className="mt-0.5 accent-primary" /><span><span className="block text-sm font-bold">Important</span><span className="text-xs text-muted-foreground">Curated and promoted for retrieval.</span></span></label>
            <label className="flex cursor-pointer items-start gap-3 rounded-xl border border-border/35 bg-background/45 p-3"><input aria-label="Mandatory memory" type="checkbox" checked={mandatory} onChange={event => setMandatory(event.target.checked)} className="mt-0.5 accent-primary" /><span><span className="block text-sm font-bold">Mandatory</span><span className="text-xs text-muted-foreground">Loaded in full at session start.</span></span></label>
          </div>
          {initial && <p className="text-xs text-muted-foreground">Classification tags are preserved during edits. Title, content, and type changes create a new traceable revision.</p>}
        </div>
        <div className="flex justify-end gap-2 border-t border-border/40 px-5 py-4"><button type="button" onClick={onClose} className="rounded-xl border border-border/45 px-4 py-2 text-xs font-bold text-muted-foreground hover:bg-accent hover:text-foreground">Cancel</button><button type="submit" disabled={!valid || saving} className="inline-flex items-center gap-2 rounded-xl bg-primary px-4 py-2 text-xs font-black text-primary-foreground disabled:opacity-50">{saving ? <LoadingSpinner size="sm" /> : <Save className="h-3.5 w-3.5" />}{initial ? 'Save revision' : 'Create memory'}</button></div>
      </form>
    </div>
  )
}

function MemoryDetail({ trace, onBack, onEdit, onRemove }: { trace: MemoryTrace; onBack: () => void; onEdit: () => void; onRemove: () => void }) {
  const [selectedKey, setSelectedKey] = useState(trace.current?.key ?? trace.revisions.at(-1)?.key ?? '')
  const versions = [
    ...(trace.current ? [trace.current] : []),
    ...[...trace.revisions].reverse(),
  ]
  const selected = versions.find(version => version.key === selectedKey) ?? versions[0]
  if (!selected) return <EmptyState icon={AlertTriangle} title="Trace unavailable" description="No authoritative versions were returned for this memory." />

  return (
    <div className="h-full overflow-y-auto">
      <header className="sticky top-0 z-20 border-b border-border/40 bg-background/90 px-5 py-4 backdrop-blur-xl">
        <button type="button" onClick={onBack} className="mb-3 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground md:hidden"><ArrowLeft className="h-3.5 w-3.5" /> Memories</button>
        <div className="flex flex-wrap items-start justify-between gap-4">
          <div className="min-w-0"><div className="mb-2 flex flex-wrap items-center gap-2"><TypeBadge type={selected.type} /><span className={cn('rounded-full border px-2 py-0.5 text-[9px] font-bold uppercase tracking-wider', selected.status === 'current' ? 'border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300' : 'border-amber-500/20 bg-amber-500/10 text-amber-700 dark:text-amber-300')}>{selected.status}</span><MemoryFlags important={selected.important} mandatory={selected.mandatory} /></div><h1 className="text-2xl font-black tracking-tight text-foreground">{selected.title}</h1><p className="mt-1 font-mono text-xs text-muted-foreground">{selected.id} · revision {selected.revision}</p></div>
          {trace.current && <div className="flex gap-2"><button type="button" onClick={onEdit} className="inline-flex items-center gap-2 rounded-xl border border-border/50 bg-card px-3 py-2 text-xs font-bold hover:bg-accent"><Edit3 className="h-3.5 w-3.5" /> Edit</button><button type="button" onClick={onRemove} className="inline-flex items-center gap-2 rounded-xl border border-rose-500/25 bg-rose-500/10 px-3 py-2 text-xs font-bold text-rose-700 hover:bg-rose-500/15 dark:text-rose-300"><Trash2 className="h-3.5 w-3.5" /> Remove</button></div>}
        </div>
      </header>

      <div className="mx-auto grid max-w-7xl gap-5 p-5 xl:grid-cols-[minmax(0,1fr)_320px]">
        <div className="min-w-0 space-y-5">
          <section className="glass-panel rounded-2xl p-5"><MemoryMarkdown content={selected.body} /></section>
          <section className="glass-panel rounded-2xl p-5"><div className="mb-4 flex items-center gap-2"><Fingerprint className="h-4 w-4 text-primary" /><h2 className="text-sm font-black uppercase tracking-wider">Authoritative metadata</h2></div><div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3"><Meta label="Scope" value={`${selected.scope} / ${selected.scope_id}`} mono /><Meta label="Associated project" value={selected.project_id} mono /><Meta label="Updated by" value={selected.updated_by} mono /><Meta label="Created" value={shortDate(selected.created_at)} /><Meta label="Updated" value={shortDate(selected.updated_at)} /><Meta label="Content hash" value={selected.content_hash} mono /><Meta label="Previous" value={selected.previous} mono /><Meta label="Next" value={selected.next} mono /><Meta label="Revision address" value={selected.revision_id || selected.key} mono /></div>{selected.tags.length > 0 && <div className="mt-4 flex flex-wrap items-center gap-2"><Tag className="h-3.5 w-3.5 text-muted-foreground" />{selected.tags.map(tag => <span key={tag} className="rounded-lg border border-border/40 bg-accent/30 px-2 py-1 font-mono text-[10px] text-muted-foreground">{tag}</span>)}</div>}</section>
        </div>

        <aside className="h-fit rounded-2xl border border-border/40 bg-card/55 p-4 xl:sticky xl:top-24"><div className="mb-4 flex items-center justify-between"><div><p className="text-[10px] font-bold uppercase tracking-[0.18em] text-primary">Traceability</p><h2 className="mt-1 text-base font-black">Revision chain</h2></div><FileClock className="h-5 w-5 text-primary" /></div><div className="relative space-y-2 before:absolute before:bottom-4 before:left-[17px] before:top-4 before:w-px before:bg-border/60">{versions.map(version => <button key={version.key} type="button" onClick={() => setSelectedKey(version.key)} className={cn('relative w-full rounded-xl border p-3 pl-10 text-left transition-colors', selected.key === version.key ? 'border-primary/40 bg-primary/10' : 'border-border/35 bg-background/45 hover:bg-accent/45')}><span className={cn('absolute left-[11px] top-4 z-10 h-3 w-3 rounded-full border-2 border-card', version.status === 'current' ? 'bg-emerald-500' : 'bg-amber-500')} /><div className="flex items-center justify-between gap-2"><span className="text-xs font-black">Revision {version.revision}</span><span className="text-[9px] font-bold uppercase tracking-wider text-muted-foreground">{version.status}</span></div><p className="mt-1 line-clamp-1 text-xs text-muted-foreground">{version.title}</p><p className="mt-1 font-mono text-[9px] text-muted-foreground/70">{shortDate(version.updated_at)}</p></button>)}</div></aside>
      </div>
    </div>
  )
}

export default function MemoryExplorerPage() {
  const navigate = useNavigate()
  const { scopeId, memoryId } = useParams<{ scopeId?: string; memoryId?: string }>()
  const scope: MemoryScope = scopeId === 'user' ? 'user' : 'project'
  const { activeProjectDir, projectName } = useAppStore()
  const [catalog, setCatalog] = useState<MemoryCatalog>({ results: [], total: 0, types: [], tags: [] })
  const [trace, setTrace] = useState<MemoryTrace | null>(null)
  const [query, setQuery] = useState('')
  const [type, setType] = useState('all')
  const [tag, setTag] = useState('all')
  const [important, setImportant] = useState('all')
  const [mandatory, setMandatory] = useState('all')
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [form, setForm] = useState<'create' | 'edit' | null>(null)
  const catalogRequest = useRef(0)
  const detailRequest = useRef(0)
  const previousProject = useRef(activeProjectDir)

  const loadCatalog = useCallback(async () => {
    const request = ++catalogRequest.current
    setLoading(true)
    try {
      const result = await memoryApi.list({ projectDir: activeProjectDir || undefined, scope, query: query.trim() || undefined, type, tag, important, mandatory })
      if (request !== catalogRequest.current) return
      const ordered = { ...result, results: sortMemoryCatalogItems(result.results) }
      setCatalog(ordered)
      if (!memoryId && ordered.results[0]) navigate(`/memory/explorer/${scope}/${encodeURIComponent(ordered.results[0].id)}`, { replace: true })
      window.document.title = `Graphit Memory — ${scope === 'user' ? 'User' : projectName || 'Project'}`
    } catch {
      if (request === catalogRequest.current) { setCatalog({ results: [], total: 0, types: [], tags: [] }); showToast('Failed to load memories', 'error') }
    } finally {
      if (request === catalogRequest.current) setLoading(false)
    }
  }, [activeProjectDir, important, mandatory, memoryId, navigate, projectName, query, scope, tag, type])

  useEffect(() => { const timer = window.setTimeout(() => { void loadCatalog() }, 180); return () => window.clearTimeout(timer) }, [loadCatalog])
  useEffect(() => {
    const request = ++detailRequest.current
    if (!memoryId) return
    memoryApi.detail(activeProjectDir || undefined, scope, memoryId)
      .then(result => { if (request === detailRequest.current) setTrace(result) })
      .catch(() => { if (request === detailRequest.current) { setTrace(null); showToast('Failed to load memory trace', 'error') } })
  }, [activeProjectDir, memoryId, scope])
  useEffect(() => {
    if (previousProject.current === activeProjectDir) return
    previousProject.current = activeProjectDir
    setTrace(null)
    navigate(`/memory/explorer/${scope}`, { replace: true })
  }, [activeProjectDir, navigate, scope])

  const selectMemory = (item: MemoryCatalogItem) => { setTrace(null); navigate(`/memory/explorer/${scope}/${encodeURIComponent(item.id)}`) }
  const showList = () => { setTrace(null); navigate(`/memory/explorer/${scope}`) }
  const changeScope = (next: MemoryScope) => { setTrace(null); setQuery(''); setType('all'); setTag('all'); setImportant('all'); setMandatory('all'); navigate(`/memory/explorer/${next}`) }

  const save = async (value: MemoryWrite | MemoryUpdate) => {
    setSaving(true)
    try {
      const result = form === 'edit' && trace?.current
        ? await memoryApi.update(activeProjectDir || undefined, scope, trace.current.id, value as MemoryUpdate)
        : await memoryApi.create(activeProjectDir || undefined, scope, value as MemoryWrite)
      setTrace(result)
      setForm(null)
      navigate(`/memory/explorer/${scope}/${encodeURIComponent(result.memory_id)}`, { replace: true })
      await loadCatalog()
      showToast(form === 'edit' ? 'Memory revision saved' : 'Memory created', 'success')
    } catch { showToast('Failed to save memory', 'error') } finally { setSaving(false) }
  }

  const remove = async () => {
    if (!trace?.current) return
    if (!window.confirm(`Remove “${trace.current.title}”? The current record will be removed and its final revision retained for traceability.`)) return
    try {
      await memoryApi.remove(activeProjectDir || undefined, scope, trace.current.id)
      setTrace(null)
      navigate(`/memory/explorer/${scope}`, { replace: true })
      await loadCatalog()
      showToast('Memory removed', 'success')
    } catch { showToast('Failed to remove memory', 'error') }
  }

  return (
    <div className="explorer-frame flex h-screen overflow-hidden bg-background text-foreground">
      <aside className={cn('w-full shrink-0 flex-col border-r border-border/40 bg-card/45 backdrop-blur-2xl md:flex md:w-[360px]', trace ? 'hidden' : 'flex')}>
        <div className="border-b border-border/40 p-4"><button type="button" onClick={() => navigate('/hub/registry')} className="mb-4 flex items-center gap-1.5 text-xs font-bold uppercase tracking-wider text-muted-foreground hover:text-foreground"><ArrowLeft className="h-3.5 w-3.5" /> Observatory</button><div className="flex items-center justify-between gap-3"><div><p className="text-[10px] font-bold uppercase tracking-[0.18em] text-primary">Memory / explorer</p><h1 className="mt-1 text-xl font-black tracking-tight">{scope === 'user' ? 'User memory' : projectName || 'Project memory'}</h1></div><div className="flex gap-1.5"><button type="button" onClick={() => setForm('create')} title="Create memory" className="rounded-xl bg-primary p-2 text-primary-foreground"><Plus className="h-4 w-4" /></button><button type="button" onClick={() => void loadCatalog()} title="Refresh memories" className="rounded-xl border border-border/40 bg-background/50 p-2 text-muted-foreground hover:text-foreground"><RefreshCw className="h-4 w-4" /></button></div></div><div className="mt-4 grid grid-cols-2 rounded-xl border border-border/40 bg-background/45 p-1"><button type="button" onClick={() => changeScope('project')} className={cn('rounded-lg px-3 py-2 text-xs font-bold', scope === 'project' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground')}>Project</button><button type="button" onClick={() => changeScope('user')} className={cn('rounded-lg px-3 py-2 text-xs font-bold', scope === 'user' ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:text-foreground')}>User</button></div><div className="relative mt-3"><Search className="absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground" /><input aria-label="Search memories" value={query} onChange={event => setQuery(event.target.value)} placeholder="Search title, content, tags…" className="w-full rounded-xl border border-border/40 bg-background/65 py-2.5 pl-9 pr-3 text-sm outline-none focus:border-primary/50" /></div><div className="mt-3 grid grid-cols-2 gap-2"><select aria-label="Filter memory type" value={type} onChange={event => setType(event.target.value)} className="min-w-0 rounded-lg border border-border/40 bg-background/65 px-2 py-2 text-xs"><option value="all">All types</option>{catalog.types.map(value => <option key={value} value={value}>{value}</option>)}</select><select aria-label="Filter memory tag" value={tag} onChange={event => setTag(event.target.value)} className="min-w-0 rounded-lg border border-border/40 bg-background/65 px-2 py-2 text-xs"><option value="all">All tags</option>{catalog.tags.map(value => <option key={value} value={value}>{value}</option>)}</select><select aria-label="Filter importance" value={important} onChange={event => setImportant(event.target.value)} className="min-w-0 rounded-lg border border-border/40 bg-background/65 px-2 py-2 text-xs"><option value="all">Any importance</option><option value="true">Important</option><option value="false">Standard</option></select><select aria-label="Filter mandatory" value={mandatory} onChange={event => setMandatory(event.target.value)} className="min-w-0 rounded-lg border border-border/40 bg-background/65 px-2 py-2 text-xs"><option value="all">Any loading</option><option value="true">Mandatory</option><option value="false">On demand</option></select></div></div>
        <div className="flex items-center justify-between px-4 py-2 text-[10px] font-bold uppercase tracking-wider text-muted-foreground"><span>Current memories</span><span>{catalog.total}</span></div>
        <div aria-label="Memory catalogue" className="flex-1 space-y-1 overflow-y-auto px-2 pb-3">{loading && <div className="flex justify-center py-10"><LoadingSpinner size="sm" /></div>}{!loading && catalog.results.map(item => <button key={item.id} type="button" onClick={() => selectMemory(item)} className={cn('w-full rounded-xl border px-3 py-3 text-left transition-colors', memoryId === item.id ? 'border-primary/35 bg-primary/10' : 'border-transparent hover:border-border/40 hover:bg-accent/35')}><div className="flex items-start justify-between gap-2"><p className="line-clamp-2 text-sm font-bold">{item.title}</p><MemoryFlags important={item.important} mandatory={item.mandatory} /></div>{item.snippet && <p className="mt-1 line-clamp-2 text-xs text-muted-foreground">{item.snippet}</p>}<div className="mt-2 flex items-center gap-2"><TypeBadge type={item.type} /><span className="font-mono text-[9px] text-muted-foreground">rev {item.revision}</span><span className="ml-auto text-[9px] text-muted-foreground">{shortDate(item.updated_at)}</span></div></button>)}{!loading && catalog.results.length === 0 && <div className="px-5 py-12 text-center"><Brain className="mx-auto h-7 w-7 text-muted-foreground/40" /><p className="mt-3 text-sm font-bold">No memories match this view</p><p className="mt-1 text-xs text-muted-foreground">Adjust filters or capture a new durable memory.</p></div>}</div>
      </aside>
      <main className={cn('min-w-0 flex-1 bg-background/95 md:block', trace ? 'block' : 'hidden')}>{trace ? <MemoryDetail key={trace.memory_id} trace={trace} onBack={showList} onEdit={() => setForm('edit')} onRemove={() => void remove()} /> : loading ? <div className="flex h-full items-center justify-center"><LoadingSpinner size="md" /></div> : <EmptyState icon={CheckCircle2} title="Select a memory" description="Choose a current memory to inspect its authoritative content and complete revision trace." />}</main>
      {form && <MemoryForm key={`${form}-${trace?.current?.key ?? 'new'}`} initial={form === 'edit' ? trace?.current : undefined} saving={saving} onClose={() => setForm(null)} onSave={value => void save(value)} />}
    </div>
  )
}
