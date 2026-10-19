import { taskApi, type TaskContext, type TaskDetail, type TaskHistoryPage, type TaskCommentsPage } from '@/api/task'
import { stringifyNativeJson } from '@/api/nativeJson'

/** Collect serialized download parts only after an explicit export, without retaining audit snapshots. */
export async function taskAuditExport(detail: TaskDetail, selection: TaskContext, signal: AbortSignal): Promise<BlobPart[]> {
 const read = async (section: 'history' | 'comments', cursor: string) => {
  signal.throwIfAborted()
  const value = await taskApi.get(selection, detail.task.specification.id, { signal, [section]: { page_size: 20, cursor } })
  signal.throwIfAborted()
  if (!value) throw new Error('The task is no longer available for export.')
  if (!value[section]) throw new Error('The requested ' + section + ' page was not returned.')
  return value
 }
 const current = { task: detail.task, creation: detail.creation, last_event: detail.last_event, generation: detail.generation, history_start_cursor: detail.history_start_cursor, comments_start_cursor: detail.comments_start_cursor }
 const parts: BlobPart[] = [stringifyNativeJson(current).slice(0, -1), ',"history":']
 let historyCursor: string | null = detail.history_start_cursor
 let first = true
 let hasEvents = false
 while (historyCursor !== null) {
  const page: TaskHistoryPage = (await read('history', historyCursor)).history!
  if (first) { parts.push('{"before":', stringifyNativeJson(page.before), ',"events":['); first = false }
  if (page.events.length) { if (hasEvents) parts.push(','); parts.push(stringifyNativeJson(page.events).slice(1, -1)); hasEvents = true }
  historyCursor = page.next_cursor
 }
 parts.push('],"next_cursor":null},"comments":{"comments":[')
 let commentsCursor: string | null = detail.comments_start_cursor
 let hasComments = false
 while (commentsCursor !== null) {
  const page: TaskCommentsPage = (await read('comments', commentsCursor)).comments!
  if (page.comments.length) { if (hasComments) parts.push(','); parts.push(stringifyNativeJson(page.comments).slice(1, -1)); hasComments = true }
  commentsCursor = page.next_cursor
 }
 parts.push('],"next_cursor":null}}')
 return parts
}
