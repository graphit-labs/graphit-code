import { sessionApi, type SessionDetail, type SessionHistoryPage } from '@/api/taskSession'
import type { TaskContext } from '@/api/task'
import { stringifyNativeJson } from '@/api/nativeJson'

/** Explicitly serialize pinned pages for a download without retaining audit snapshots. */
export async function sessionAuditExport(detail: SessionDetail, selection: TaskContext, signal: AbortSignal): Promise<BlobPart[]> {
 const current = { session: detail.session, creation: detail.creation, last_event: detail.last_event, generation: detail.generation }
 const parts: BlobPart[] = [stringifyNativeJson(current).slice(0, -1), ',"history":']
 let cursor: string | null = detail.history_start_cursor
 let first = true
 let hasEvents = false
 while (cursor !== null) {
  signal.throwIfAborted()
  const value = await sessionApi.get(selection, detail.session.id, { signal, history: { page_size: 20, cursor } })
  signal.throwIfAborted()
  if (!value) throw new Error('The session is no longer available for export.')
  const page: SessionHistoryPage | null = value.history
  if (!page) throw new Error('The requested session history page was not returned.')
  if (first) { parts.push('{"before":', stringifyNativeJson(page.before), ',"events":['); first = false }
  if (page.events.length) { if (hasEvents) parts.push(','); parts.push(stringifyNativeJson(page.events).slice(1, -1)); hasEvents = true }
  cursor = page.next_cursor
 }
 parts.push('],"next_cursor":null}}')
 return parts
}
