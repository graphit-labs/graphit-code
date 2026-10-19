import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { taskApi, type CatalogueCursor, type CatalogueFilter, type TaskContext, type TaskStatus, type TaskSummary, type TaskSearchCursor, type TaskSearchMode, type TaskSearchMatches } from '@/api/task'
import { invalidateTaskWorkspace, taskContextKey, useTaskWorkspace } from './taskWorkspace'

interface CatalogueState { key: string; selection: TaskContext | null; tasks: TaskSummary[]; cursor: CatalogueCursor | TaskSearchCursor | null; totalHits?: number; matches?: TaskSearchMatches; error: string }
const empty: CatalogueState = { key: '', selection: null, tasks: [], cursor: null, error: '' }
export function useTaskCatalogue(query: string, status: TaskStatus | 'all', flagged?: boolean, blocked?: boolean, searchMode?: TaskSearchMode) {
 const { selection, phase } = useTaskWorkspace()
 const ready = phase === 'ready' && selection !== null
 const targetKey = taskContextKey(selection)
 const [state, setState] = useState<CatalogueState>(empty)
 const trimmedQuery = query.trim()
 const filter = useMemo<CatalogueFilter>(() => ({ ...(trimmedQuery ? { query: trimmedQuery } : {}), ...(status === 'all' ? {} : { status }), ...(flagged === undefined ? {} : { flagged }), ...(blocked === undefined ? {} : { blocked }) }), [trimmedQuery, status, flagged, blocked])
 const searching = Boolean(trimmedQuery && searchMode)
 const key = JSON.stringify([targetKey, filter, searchMode])
 const [moreKey, setMoreKey] = useState<string | null>(null)
 const [busyKey, setBusyKey] = useState<string | null>(null)
 const reader = useRef<AbortController | null>(null)
 const more = useRef<AbortController | null>(null)
 const scheduled = useRef<number | null>(null)
 const refresh = useCallback(async () => {
  if (scheduled.current !== null) window.clearTimeout(scheduled.current)
  scheduled.current = null
  reader.current?.abort(); more.current?.abort()
  if (!ready || !selection) return
  const controller = new AbortController(); reader.current = controller; setBusyKey(key)
  try {
   const admitted = await taskApi.context(selection, controller.signal)
   const page = searching ? await taskApi.search({ selection: admitted, text: trimmedQuery, mode: searchMode, filter: { ...(status === 'all' ? {} : { status }), ...(flagged === undefined ? {} : { flagged }), ...(blocked === undefined ? {} : { blocked }) }, signal: controller.signal }) : await taskApi.catalogue({ selection: admitted, filter, signal: controller.signal })
   if (!controller.signal.aborted) setState(previous => ({ key, selection: previous.key === key && taskContextKey(previous.selection) === taskContextKey(admitted) ? previous.selection : admitted, tasks: page.results, cursor: page.next_cursor, totalHits: 'total_hits' in page ? page.total_hits : undefined, matches: 'matches' in page ? page.matches : undefined, error: '' }))
  } catch (cause) {
   if (!controller.signal.aborted) {
    invalidateTaskWorkspace(cause)
    setState({ ...empty, key, error: cause instanceof Error ? cause.message : 'Unable to read tasks' })
    throw cause
   }
  } finally { if (!controller.signal.aborted) setBusyKey(null) }
 }, [ready, selection, key, filter, searching, trimmedQuery, searchMode, status, flagged, blocked])
 useEffect(() => {
  more.current?.abort()
  const timer = window.setTimeout(() => {
   if (scheduled.current === timer) scheduled.current = null
   void refresh().catch(() => {})
  }, 200)
  scheduled.current = timer
  return () => {
   window.clearTimeout(timer)
   if (scheduled.current === timer) scheduled.current = null
   reader.current?.abort(); more.current?.abort()
  }
 }, [refresh])
 const current = state.key === key && ready ? state : { ...empty, key }
 const loading = ready && (state.key !== key || busyKey === key)
 const loadingMore = moreKey === key
 const loadMore = async () => {
  if (!current.selection || !current.cursor || loadingMore) return
  const controller = new AbortController(); more.current = controller; setMoreKey(key)
  try {
   const page = searching ? await taskApi.search({ selection: current.selection, text: trimmedQuery, mode: searchMode, filter: { ...(status === 'all' ? {} : { status }), ...(flagged === undefined ? {} : { flagged }), ...(blocked === undefined ? {} : { blocked }) }, cursor: current.cursor as TaskSearchCursor, signal: controller.signal }) : await taskApi.catalogue({ selection: current.selection, filter, cursor: current.cursor as CatalogueCursor, signal: controller.signal })
   if (!controller.signal.aborted) setState(previous => previous.key === key ? { ...previous, tasks: [...new Map([...previous.tasks, ...page.results].map(task => [task.id, task])).values()], cursor: page.next_cursor, matches: 'matches' in page ? { ...previous.matches, ...page.matches } : undefined, error: '' } : previous)
  } catch (cause) {
   if (!controller.signal.aborted) { invalidateTaskWorkspace(cause); setState(previous => previous.key === key ? { ...empty, key, error: cause instanceof Error ? cause.message : 'Unable to load next page' } : previous) }
  } finally { if (!controller.signal.aborted) setMoreKey(null) }
 }
 return { ...current, targetKey, ready, loading, loadingMore, loadMore, refresh }
}
