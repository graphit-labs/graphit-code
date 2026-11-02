import { projectReferenceBinding } from '@/api/nativeRecordReferences'
import { referenceDestinationError } from '@/api/nativeReferenceNavigation'
import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useLocation, useNavigate, useParams } from 'react-router-dom'
import { Download, Circle, CheckCircle2, XCircle } from 'lucide-react'
import { taskApi, type TaskContext, type TaskDetail, type TaskStatus, type TaskSearchMode } from '@/api/task'
import { nativeTimestamp as timestamp, stringifyNativeJson } from '@/api/nativeJson'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { WorkPage, WorkHeader, WorkSection, WorkNotice, WorkEmpty, WorkStatusBadge, WorkBadge, FactList } from '@/components/shared/EngineeringUI'
import { RecordReferences } from '@/components/shared/RecordReferences'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { MarkdownContent } from '@/components/wiki/WikiMarkdown'
import { refreshAll, usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { invalidateTaskWorkspace } from './taskWorkspaceState'
import { TaskModeToggle } from './TaskModeToggle'
import { TaskFilters } from './TaskFilters'
import { useTaskCatalogue } from './useTaskCatalogue'
import { TaskWorkspaceNotice } from './TaskWorkspace'
import { scheduleLabel } from './taskTimelineDates'
import { TaskComments, TaskHistory } from './TaskAuditSections'
import { taskAuditExport } from './taskAuditExport'

const rich = (content: string) => <MarkdownContent content={content} />
function download(parts: BlobPart[], filename: string) {
 const url = URL.createObjectURL(new Blob(parts, { type: 'application/json' }))
 const anchor = document.createElement('a'); anchor.href = url; anchor.download = filename; anchor.click(); URL.revokeObjectURL(url)
}
function Dossier({ detail, selection, onBack }: { detail: TaskDetail; selection: TaskContext; onBack: () => void }) {
 const location = useLocation()
 const { task } = detail
 const exporter = useRef<AbortController | null>(null)
 const [exporting, setExporting] = useState(false)
 const [exportError, setExportError] = useState('')
 useEffect(() => () => exporter.current?.abort(), [])
 const exportTask = async () => {
  exporter.current?.abort()
  const controller = new AbortController(); exporter.current = controller
  setExporting(true); setExportError('')
  try {
   const parts = await taskAuditExport(detail, selection, controller.signal)
   if (!controller.signal.aborted) download(parts, task.specification.id + '.json')
  } catch (cause) {
   if (!controller.signal.aborted) { invalidateTaskWorkspace(cause); setExportError(cause instanceof Error ? cause.message : 'Unable to export task') }
  } finally { if (!controller.signal.aborted) setExporting(false) }
 }
 const spec = task.specification
 const binding = projectReferenceBinding(selection, 'task', spec.id)
 const destinationError = referenceDestinationError(location.search, binding, true)
 if (destinationError) return <WorkNotice title="Reference destination unavailable" tone="error">{destinationError}</WorkNotice>
 const activeChecks = spec.checks.filter(check => check.status !== 'superseded')
 return <article className="work-dossier" aria-label="Task dossier">
 <header className="dossier-header"><div><div className="dossier-status"><WorkStatusBadge status={task.status} /><span>Priority {spec.priority}</span>{task.flagged && <span>Flagged</span>}<Link className="work-badge" title="Open linked session" to={projectRecordHref('session', task.session_id, selection)}>Session</Link></div><h2>{spec.title}</h2><small><code>{spec.id}</code> / revision {spec.revision} / {spec.task_type}</small></div><div className="work-actions"><button className="work-button" onClick={onBack}>Tasks</button><button className="work-button" disabled={exporting} aria-busy={exporting} onClick={() => void exportTask()}><Download size={15} />{exporting ? 'Exporting…' : 'Export complete JSON'}</button></div></header>
 {exportError && <WorkNotice title="Unable to export task" tone="error">{exportError}</WorkNotice>}
 <p className="work-panel">Commit: {task.commit ? <code className="break-all">{task.commit}</code> : <span>No commit associated</span>}</p>
 <nav className="dossier-nav" aria-label="Task record sections"><a href="#task-evidence">Checks & evidence</a><a href="#task-scope">Specification</a><a href="#task-comments">Comments</a><a href="#task-history">Lifecycle</a><a href="#task-revisions">Revisions</a></nav>
 <div className="dossier-layout"><div className="dossier-content">
 {task.progress?.next_step && <WorkNotice title="Next step">{rich(task.progress.next_step)}</WorkNotice>}
 {task.progress?.summary && <WorkSection title="Latest progress">{rich(task.progress.summary)}</WorkSection>}
 <WorkSection title="Schedule"><p>{scheduleLabel(spec.schedule)}</p></WorkSection>
 <WorkSection id="task-evidence" title="Checks" actions={<WorkBadge>{activeChecks.filter(check => check.status === 'passed').length} / {activeChecks.length} active checks passed</WorkBadge>}>
 <ul className="evidence-list">{spec.checks.map(check => <li key={check.id}>{check.status === 'passed' ? <CheckCircle2 size={18} /> : check.status === 'failed' ? <XCircle size={18} /> : <Circle size={18} />}<div><small>{check.kind} · {check.status}</small>{rich(check.text)}{check.verification && <div className="evidence-body"><strong>Evidence</strong>{rich(check.verification.evidence)}<small>{check.verification.verified_by} / {timestamp(check.verification.occurred_at)}</small></div>}{check.supersession && <WorkNotice title="Superseded check">{rich(check.supersession.reason)}{check.supersession.replacement && <code>{check.supersession.replacement}</code>}</WorkNotice>}</div></li>)}</ul>{!spec.checks.length && <p>No checks recorded.</p>}
 </WorkSection>
 <WorkSection id="task-scope" title="Specification">{rich(spec.description)}</WorkSection>
 {task.depends_on.length > 0 && <WorkSection title="Dependencies"><ul>{task.depends_on.map(id => <li key={id}><Link to={projectRecordHref('task', id, selection)}>{id}</Link></li>)}</ul></WorkSection>}
 <TaskComments detail={detail} selection={selection} />
 <TaskHistory detail={detail} selection={selection} />
 <details className="work-disclosure"><summary>Current JSON document</summary><pre className="work-code">{stringifyNativeJson(detail, 2)}</pre></details>
 </div><aside className="dossier-context"><h3>Accountability</h3><FactList items={[["Owner", task.ownership.owner || 'Unclaimed'], ["Created", timestamp(task.created_at)], ["Updated", timestamp(task.updated_at)], ["Lease", timestamp(task.ownership.lease_expires_at)], ["Claim epoch", task.ownership.claim_epoch]]} /><Link className="work-button" to={projectRecordHref('session', task.session_id, selection)}>Open session</Link>{task.parent_id && <Link to={projectRecordHref('task', task.parent_id, selection)}>Parent {task.parent_id}</Link>}<RecordReferences binding={binding} /></aside></div>
 </article>
}
export default function TaskExplorerPage() {
 const { taskId } = useParams<{ taskId: string }>()
 const navigate = useNavigate()
 const [query, setQuery] = useState('')
 const [searchMode, setSearchMode] = useState<TaskSearchMode>('fts')
 const [status, setStatus] = useState<TaskStatus | 'all'>('all')
 const [flagged, setFlagged] = useState<boolean | undefined>(undefined)
 const [blocked, setBlocked] = useState<boolean | undefined>(undefined)
 const catalogue = useTaskCatalogue(query, status, flagged, blocked, searchMode)
 const [closedKey, setClosedKey] = useState<string | null>(null)
 const [result, setResult] = useState<{ key: string; detail: TaskDetail | null; error: string } | null>(null)
 const [detailRevision, setDetailRevision] = useState(0)
 const [exportingKey, setExportingKey] = useState<string | null>(null)
 const [exportError, setExportError] = useState('')
 const exporter = useRef<AbortController | null>(null)
 const detailReader = useRef<AbortController | null>(null)
 const selected = taskId ?? (closedKey === catalogue.key ? '' : catalogue.tasks[0]?.id ?? '')
 const requestKey = JSON.stringify([catalogue.key, selected, detailRevision])
 const currentResult = catalogue.selection && catalogue.ready && result?.key === requestKey ? result : null
 const detail = currentResult?.detail ?? null
 const detailError = currentResult?.error ?? ''
 const detailLoading = Boolean(selected && catalogue.selection && !currentResult)
 const exporting = exportingKey === catalogue.targetKey
 const catalogueHref = catalogue.selection ? '/task/explorer?' + new URLSearchParams({ selection: stringifyNativeJson(catalogue.selection) }).toString() : null
 const previousSelection = useRef(catalogue.targetKey)
 useEffect(() => {
  if (catalogueHref && previousSelection.current !== catalogue.targetKey) { const previous = previousSelection.current; previousSelection.current = catalogue.targetKey; if (previous !== 'null') navigate(catalogueHref, { replace: true }) }
 }, [catalogue.targetKey, catalogueHref, navigate])
 useEffect(() => () => exporter.current?.abort(), [catalogue.targetKey])
 const readDetail = useCallback(async () => {
  detailReader.current?.abort()
  const controller = new AbortController(); detailReader.current = controller
  if (!selected || !catalogue.selection) return
  try {
   const value = await taskApi.get(catalogue.selection, selected, { signal: controller.signal })
   if (!controller.signal.aborted) setResult({ key: requestKey, detail: value, error: '' })
  } catch (cause) {
   if (!controller.signal.aborted) {
    invalidateTaskWorkspace(cause)
    setResult({ key: requestKey, detail: null, error: cause instanceof Error ? cause.message : 'Unable to read task' })
    throw cause
   }
  }
 }, [catalogue.selection, requestKey, selected])
 useEffect(() => {
  let active = true
  queueMicrotask(() => { if (active) void readDetail().catch(() => {}) })
  return () => { active = false; detailReader.current?.abort() }
 }, [readDetail])
 const refreshCatalogue = catalogue.refresh
 const refresh = useCallback(() => refreshAll([refreshCatalogue(), readDetail()]), [refreshCatalogue, readDetail])
 usePageRefresh(refresh)
 const exportAll = async () => {
  if (!catalogue.selection) return
  exporter.current?.abort(); const controller = new AbortController(); exporter.current = controller; setExportingKey(catalogue.targetKey); setExportError('')
  try {
   const parts: BlobPart[] = ['{"selection":', stringifyNativeJson(catalogue.selection), ',"tasks":[']
   let hasRecords = false
   let cursor: import('@/api/task').CatalogueCursor | null = null
   do {
    const page = await taskApi.catalogue({ selection: catalogue.selection, page_size: 20, cursor, signal: controller.signal })
    for (const item of page.results) {
     controller.signal.throwIfAborted()
     const record = await taskApi.get(catalogue.selection, item.id, { signal: controller.signal })
     if (!record) throw new Error('A task is no longer available for export. Retry the export.')
     const recordParts = await taskAuditExport(record, catalogue.selection, controller.signal)
     if (hasRecords) parts.push(',')
     for (const part of recordParts) parts.push(part)
     hasRecords = true
    }
    cursor = page.next_cursor
   } while (cursor && !controller.signal.aborted)
   parts.push(']}')
   if (!controller.signal.aborted) download(parts, 'graphit-tasks.json')
  } catch (error) { if (!controller.signal.aborted) { invalidateTaskWorkspace(error); setExportError(error instanceof Error ? error.message : 'Unable to export tasks') } }
  finally { if (!controller.signal.aborted) setExportingKey(null) }
 }
 return <WorkPage><WorkHeader title="Tasks & evidence" description="Inspect the work contract, its ownership and recorded evidence." /><TaskModeToggle mode="tasks" />
 {!catalogue.ready ? <TaskWorkspaceNotice /> : <>
 <div className="work-toolbar"><TaskFilters searchMode={searchMode} onSearchMode={setSearchMode} query={query} onQuery={setQuery} status={status} onStatus={setStatus} flagged={flagged} onFlagged={setFlagged} blocked={blocked} onBlocked={setBlocked} /><button className="work-button" onClick={() => void refresh().catch(() => {})}>Refresh</button><button className="work-button" disabled={exporting || !catalogue.selection} onClick={() => void exportAll()}><Download size={15} />Export all tasks</button></div>
 {(catalogue.error || exportError) && <WorkNotice tone="error" title="Unable to read tasks">{catalogue.error || exportError}<button className="work-button" onClick={() => void refresh().catch(() => {})}>Retry</button></WorkNotice>}
 {query.trim() && searchMode !== 'fts' && <p className="work-muted">Semantic results become available as background embedding advances. Hybrid also includes text matches.</p>}
 <div className="work-catalogue-summary"><span>{catalogue.tasks.length}{catalogue.cursor ? '+' : ''} {catalogue.totalHits === undefined ? 'tasks in loaded pages' : `of ${catalogue.totalHits} matching tasks`}</span><span>Select a record to review its delivery contract</span></div>
 <div className="work-table-wrap work-catalogue" aria-label="Task catalogue"><table className="work-table"><thead><tr><th>Work item</th><th>Status</th><th>Owner</th><th>Priority</th><th>Readiness</th></tr></thead><tbody>{catalogue.tasks.map(task => { const match = catalogue.matches?.[task.id]; return <tr key={task.id} className={selected === task.id ? 'selected' : ''}><td><button className="record-title" onClick={() => { if (selected === task.id && !detail) setDetailRevision(value => value + 1); if (catalogue.selection) navigate(projectRecordHref('task', task.id, catalogue.selection), { replace: true }) }}>{task.title}</button><small><code>{task.id}</code> <Link title="Open linked session" to={catalogue.selection ? projectRecordHref('session', task.session_id, catalogue.selection) : '/task/sessions'}>Session</Link></small>{match && <small aria-label={'Matched location for ' + task.title}><Link title={'Search score ' + match.score.toFixed(3)} to={(catalogue.selection ? projectRecordHref('task', task.id, catalogue.selection) : '/task/explorer') + ('Comment' in match.location ? '#task-comments' : '#task-scope')}>{'Comment' in match.location ? 'Comment #' + match.location.Comment.sequence : 'Specification r' + match.location.Specification.revision}</Link></small>}</td><td><WorkStatusBadge status={task.status} /></td><td>{task.ownership.owner || 'Unclaimed'}</td><td>P{task.priority}</td><td>{task.flagged ? 'Flagged' : task.ready ? 'Ready' : 'Not ready'}</td></tr>})}</tbody></table>{catalogue.loading && <LoadingSpinner label="Loading tasks…" />}{!catalogue.loading && !catalogue.tasks.length && !catalogue.error && <WorkEmpty title="No tasks match this view.">Change a filter or choose a project with recorded work.</WorkEmpty>}{catalogue.cursor && <button className="work-button" disabled={catalogue.loadingMore} onClick={() => void catalogue.loadMore()}>{catalogue.loadingMore ? 'Loading…' : 'Load more'}</button>}</div>
 {detailError && <WorkNotice tone="error" title="Unable to read task">{detailError}<button className="work-button" onClick={() => setDetailRevision(value => value + 1)}>Retry task</button></WorkNotice>}
 {detailLoading ? <LoadingSpinner label="Loading task…" /> : detail && catalogue.selection ? <Dossier key={requestKey + stringifyNativeJson(detail.generation)} detail={detail} selection={catalogue.selection} onBack={() => { setClosedKey(catalogue.key); if (catalogueHref) navigate(catalogueHref, { replace: true }) }} /> : currentResult && selected && !detailError ? <WorkEmpty title="Task not found">This task is unavailable in the selected project and version.</WorkEmpty> : <WorkEmpty title="Select a task">Choose a task to inspect its native record.</WorkEmpty>}
 </>}
 </WorkPage>
}
