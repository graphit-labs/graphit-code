import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useNavigate, useParams } from 'react-router-dom'
import { sessionApi, type SessionDetail, type SessionHead } from '@/api/taskSession'
import { taskApi, type CatalogueCursor, type TaskContext, type TaskStatus, type TaskSummary } from '@/api/task'
import { nativeTimestamp as timestamp, stringifyNativeJson } from '@/api/nativeJson'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { WorkPage, WorkHeader, WorkSection, WorkStatusBadge, WorkNotice, WorkEmpty, WorkSearch, SessionTaskProgress } from '@/components/shared/EngineeringUI'
import { StyledSelect } from '@/components/shared/StyledSelect'
import { LoadingSpinner } from '@/components/shared/LoadingSpinner'
import { refreshAll, usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { useAppStore } from '@/store/appStore'
import { invalidateTaskWorkspace, taskContextKey, useTaskWorkspace } from './taskWorkspace'
import { TaskWorkspaceNotice } from './TaskWorkspace'
import { TaskModeToggle } from './TaskModeToggle'
import { SessionDossier } from './SessionDossier'
const browserIdentity = () => { const state = useAppStore.getState(); return JSON.stringify([state.apiBase, state.webIdentity]) }
export default function SessionExplorerPage() {
 const { sessionId } = useParams<{ sessionId: string }>()
 const navigate = useNavigate()
 const { selection: workspaceSelection, phase } = useTaskWorkspace()
 const browserKey = useAppStore(state => JSON.stringify([state.apiBase, state.webIdentity]))
 const ready = phase === 'ready' && workspaceSelection !== null
 const scopeKey = taskContextKey(workspaceSelection)
 const [query, setQuery] = useState('')
 const [status, setStatus] = useState<TaskStatus | 'all'>('all')
 const [activeOnly, setActiveOnly] = useState(false)
 const [refresh, setRefresh] = useState(0)
 const key = JSON.stringify([scopeKey, browserKey, refresh])
 const live = useCallback(() => browserIdentity() === browserKey && taskContextKey(useTaskWorkspace.getState().selection) === scopeKey && useTaskWorkspace.getState().phase === 'ready', [browserKey, scopeKey])
 const [closedKey, setClosedKey] = useState<string | null>(null)
 const [catalogue, setCatalogue] = useState<{ key: string; heads: SessionHead[]; selection: TaskContext } | null>(null)
 const [detail, setDetail] = useState<{ key: string; id: string; value: SessionDetail | null; error: string; serial: number } | null>(null)
 const [linked, setLinked] = useState<{ key: string; id: string; tasks: TaskSummary[]; cursor: CatalogueCursor | null; loading: boolean; error: string; retry: 'initial' | 'next' } | null>(null)
 const [failure, setFailure] = useState<{ key: string; message: string } | null>(null)
 const reader = useRef<AbortController | null>(null)
 const linkedReader = useRef<AbortController | null>(null)
 const serial = useRef(0)
 const previousScope = useRef(scopeKey)
 const headReader = useRef<AbortController | null>(null)
 const readHeads = useCallback(async () => {
  headReader.current?.abort()
  const controller = new AbortController(); headReader.current = controller
  if (!ready || !workspaceSelection) return
  try {
   const selection = await taskApi.context(workspaceSelection, controller.signal)
   if (controller.signal.aborted || !live()) return
   const heads = await sessionApi.heads(selection, controller.signal)
   if (!controller.signal.aborted && live()) {
    setCatalogue(previous => ({ key, selection: previous?.key === key && taskContextKey(previous.selection) === taskContextKey(selection) ? previous.selection : selection, heads }))
    setFailure(null)
   }
  } catch (cause) {
   if (!controller.signal.aborted && live()) {
    invalidateTaskWorkspace(cause); setCatalogue(null)
    setFailure({ key, message: cause instanceof Error ? cause.message : 'Unable to read sessions' })
    throw cause
   }
  }
 }, [ready, key, workspaceSelection, live])
 useEffect(() => { void readHeads().catch(() => {}); return () => headReader.current?.abort() }, [readHeads])
 const current = catalogue?.key === key && ready ? catalogue : null
 const selected = sessionId ?? (closedKey === key ? '' : current?.heads[0]?.id ?? '')
 const error = failure?.key === key ? failure.message : ''
 const loading = ready && !current && !error
 const currentResult = current && detail?.key === key && detail.id === selected ? detail : null
 const currentDetail = currentResult?.value ?? null
 const detailError = currentResult?.error ?? ''
 const detailLoading = Boolean(current && selected && !currentResult)
 const currentLinked = linked?.key === key && linked.id === selected ? linked : null
 const currentSelection = current?.selection
 const catalogueHref = currentSelection ? '/task/sessions?' + new URLSearchParams({ selection: stringifyNativeJson(currentSelection) }).toString() : null
 useEffect(() => {
  if (catalogueHref && previousScope.current !== scopeKey) { const previous = previousScope.current; previousScope.current = scopeKey; if (previous !== 'null') navigate(catalogueHref, { replace: true }) }
 }, [scopeKey, catalogueHref, navigate])
 const readSession = useCallback(async () => {
  reader.current?.abort()
  const controller = new AbortController(); reader.current = controller
  const request = ++serial.current
  if (!currentSelection || !selected) return
  try {
   const value = await sessionApi.get(currentSelection, selected, { signal: controller.signal })
   if (!controller.signal.aborted && live()) setDetail({ key, id: selected, value, error: '', serial: request })
  } catch (cause) {
   if (!controller.signal.aborted && live()) {
    invalidateTaskWorkspace(cause)
    setDetail({ key, id: selected, value: null, error: cause instanceof Error ? cause.message : 'Unable to read session', serial: request })
    throw cause
   }
  }
 }, [currentSelection, selected, key, live])
 useEffect(() => { void readSession().catch(() => {}); return () => reader.current?.abort() }, [readSession])
 const readLinked = useCallback(async () => {
  linkedReader.current?.abort()
  const controller = new AbortController(); linkedReader.current = controller
  if (!currentSelection || !selected) return
  setLinked(previous => ({ key, id: selected, tasks: previous?.key === key && previous.id === selected ? previous.tasks : [], cursor: previous?.key === key && previous.id === selected ? previous.cursor : null, loading: true, error: '', retry: 'initial' }))
  try {
   const page = await taskApi.catalogue({ selection: currentSelection, filter: { session_id: selected }, signal: controller.signal })
   if (!controller.signal.aborted && live()) setLinked({ key, id: selected, tasks: page.results, cursor: page.next_cursor, loading: false, error: '', retry: 'initial' })
  } catch (cause) {
   if (!controller.signal.aborted && live()) {
    invalidateTaskWorkspace(cause)
    setLinked(previous => previous && previous.key === key && previous.id === selected ? { ...previous, loading: false, error: cause instanceof Error ? cause.message : 'Unable to read linked tasks', retry: 'initial' } : previous)
    throw cause
   }
  }
 }, [currentSelection, selected, key, live])
 useEffect(() => { void readLinked().catch(() => {}); return () => linkedReader.current?.abort() }, [readLinked])
 const refreshPage = useCallback(() => refreshAll([readHeads(), readSession(), readLinked()]), [readHeads, readSession, readLinked])
 usePageRefresh(refreshPage)
 const loadMore = async () => {
  const controller = linkedReader.current
  if (!current || !currentLinked?.cursor || !controller || currentLinked.loading) return
  setLinked(previous => previous ? { ...previous, loading: true, error: '' } : previous)
  try {
   const page = await taskApi.catalogue({ selection: current.selection, filter: { session_id: selected }, cursor: currentLinked.cursor, signal: controller.signal })
   if (!controller.signal.aborted && live()) setLinked(previous => previous ? { ...previous, tasks: [...new Map([...previous.tasks, ...page.results].map(task => [task.id, task])).values()], cursor: page.next_cursor, loading: false, error: '' } : null)
  } catch (cause) {
   if (!controller.signal.aborted && live()) {
    invalidateTaskWorkspace(cause)
    setLinked(previous => previous ? { ...previous, loading: false, error: cause instanceof Error ? cause.message : 'Unable to read linked tasks', retry: 'next' } : previous)
   }
  }
 }
 const heads = (current?.heads ?? []).filter(head => (!query.trim() || (head.title + ' ' + head.id).toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())) && (status === 'all' || head.status === status) && (!activeOnly || head.status === 'open' || head.status === 'in_progress'))
 return <WorkPage><WorkHeader title="Sessions" description="Resume the request with its decisions, ownership and next action intact." /><TaskModeToggle mode="sessions" />
 {!ready ? <TaskWorkspaceNotice /> : <>
 <div className="work-toolbar"><WorkSearch label="Search session titles" value={query} onChange={setQuery} placeholder="Filter loaded titles or IDs" /><StyledSelect aria-label="Filter session status" menuLabel="Session statuses" value={status} onChange={event => setStatus(event.target.value as TaskStatus | 'all')}><option value="all">All statuses</option><option value="open">Open</option><option value="in_progress">In progress</option><option value="completed">Completed</option><option value="cancelled">Cancelled</option></StyledSelect><button className="work-button" aria-pressed={activeOnly} onClick={() => setActiveOnly(value => !value)}>Active only</button><button className="work-button" onClick={() => setRefresh(value => value + 1)}>Refresh</button></div>
 {error && <WorkNotice tone="error" title="Unable to read sessions">{error}<button className="work-button" onClick={() => setRefresh(value => value + 1)}>Retry</button></WorkNotice>}
 <div className="work-catalogue-summary"><span>{heads.length} of {current?.heads.length ?? 0} loaded session summaries</span><span>Current state loads on selection; load history explicitly.</span></div>
 <div className="work-table-wrap work-catalogue" aria-label="Session catalogue"><table className="work-table"><thead><tr><th>Request</th><th>Status</th><th>Tasks</th><th>Coordinator</th><th>Updated</th></tr></thead><tbody>{heads.map(head => <tr key={head.id} className={selected === head.id ? 'selected' : ''}><td><button className="record-title" onClick={() => { if (current) navigate(projectRecordHref('session', head.id, current.selection), { replace: true }) }}>{head.title}</button><small><code>{head.id}</code></small></td><td><WorkStatusBadge status={head.status} /></td><td><SessionTaskProgress completed={head.completed_tasks} total={head.total_tasks} /></td><td>{head.ownership.owner || 'Unclaimed'}</td><td>{timestamp(head.updated_at)}</td></tr>)}</tbody></table>{loading && <LoadingSpinner label="Loading sessions…" />}{!loading && !heads.length && !error && <WorkEmpty title="No sessions match this view.">Try another title, status or project.</WorkEmpty>}</div>
 {detailError && <WorkNotice tone="error" title="Unable to read session">{detailError}<button className="work-button" onClick={() => setRefresh(value => value + 1)}>Retry session</button></WorkNotice>}
 {detailLoading ? <LoadingSpinner label="Loading session…" /> : currentDetail && currentSelection ? <SessionDossier key={stringifyNativeJson([key, selected, currentResult?.serial, currentDetail.generation])} detail={currentDetail} selection={currentSelection} onBack={() => { setClosedKey(key); if (catalogueHref) navigate(catalogueHref, { replace: true }) }} linkedTasks={
  <WorkSection id="session-work" title="Linked tasks">
   <p>{currentLinked?.tasks.length ?? 0}{currentLinked?.cursor ? '+' : ''} tasks in loaded pages</p>
   {(!currentLinked || currentLinked.loading) && <LoadingSpinner label="Loading linked tasks…" />}
   {currentLinked?.error && <WorkNotice title="Unable to read linked tasks" tone="error">{currentLinked.error}<button className="work-button" disabled={currentLinked.loading} onClick={() => void (currentLinked.retry === 'next' ? loadMore() : readLinked().catch(() => {}))}>Retry linked tasks</button></WorkNotice>}
   {currentLinked?.tasks.map(task => <p key={task.id}><Link className="record-title" to={projectRecordHref('task', task.id, currentSelection)}>{task.title}</Link> <WorkStatusBadge status={task.status} /></p>)}
   {currentLinked?.cursor && <button className="work-button" disabled={currentLinked.loading} onClick={() => void loadMore()}>{currentLinked.loading ? 'Loading…' : 'Load more linked tasks'}</button>}
   {currentLinked && !currentLinked.loading && !currentLinked.error && !currentLinked.tasks.length && <p>No tasks are linked to this session yet.</p>}
  </WorkSection>
 } /> : currentResult && !detailError ? <WorkEmpty title="Session unavailable">The selected session was not returned by the authorized reader.</WorkEmpty> : <WorkEmpty title="Select a session">Choose a session to inspect its evolving request and linked tasks.</WorkEmpty>}
 </>}
 </WorkPage>
}
