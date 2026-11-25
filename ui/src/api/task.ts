import { getApiBase } from '@/lib/utils'
import { parseNativeJson, stringifyNativeJson, type NativeInteger } from './nativeJson'

export type TaskStatus = 'open' | 'in_progress' | 'completed' | 'cancelled'
export interface TaskSelection { project_dir?: string; project_id?: string; project_version?: string }
export interface TaskContext { project_dir: string; project_id: string; project_version: string }
/** Display metadata from the authorized inventory; never part of a selection. */
export interface TaskProjectMetadata { project_id: string; name: string }
export interface TaskSchedule { start_date?: string; end_date?: string; due_date?: string }
export interface TaskTimelineItem { id: string; title: string; status: TaskStatus; depends_on: readonly string[]; schedule: TaskSchedule | null }
export interface Ownership { owner: string | null; claim_epoch: NativeInteger; claimed_at: NativeInteger | null; heartbeat_at: NativeInteger | null; lease_expires_at: NativeInteger | null }
export interface ProgressObservation { kind: 'progress' | 'checkpoint'; sequence: NativeInteger; observed_at: NativeInteger }
export interface Progress { observation: ProgressObservation; summary: string; next_step: string }
export interface TaskCheck {
 id: string; kind: 'acceptance' | 'test'; text: string; status: 'pending' | 'passed' | 'failed' | 'superseded';
 verification: { evidence: string; verified_by: string; occurred_at: NativeInteger } | null;
 supersession: { actor: string; reason: string; occurred_at: NativeInteger; replacement: string | null } | null
}
export interface TaskSpecification { id: string; revision: NativeInteger; title: string; description: string; task_type: string; priority: number; schedule: TaskSchedule | null; checks: TaskCheck[] }
export interface Task {
 commit?: string | null;
 specification: TaskSpecification; session_id: string; parent_id: string | null; depends_on: string[]; idempotency_key: string;
 status: TaskStatus; flagged: boolean; ownership: Ownership; progress: Progress | null; created_at: NativeInteger; updated_at: NativeInteger
}
export type LifecycleKind = 'claimed' | 'renewed' | 'progress' | 'released' | 'completed' |
 { revised: { reason: string } } | { check_verified: { check_id: string } } |
 { check_superseded: { check_id: string; replacement: string | null; reason: string } } |
 { commit_updated: { reason: string } } | { links_updated: { reason: string } } | { flag_updated: { flagged: boolean; reason: string } } | { cancelled: { reason: string } } | { comment_added: { id: string; sequence: number } }
export interface TaskEvent { actor: string; kind: LifecycleKind; observed_at: NativeInteger; record: Task }
export interface TaskComment { id: string; task_id: string; session_id: string; idempotency_key: string; sequence: NativeInteger; kind: 'note' | 'decision' | 'problem' | 'lesson' | 'knowledge'; body: string; actor: string; at: NativeInteger; revision: NativeInteger }
export interface TaskHistoryPage { before: Task; events: TaskEvent[]; next_cursor: string | null }
export interface TaskCommentsPage { comments: TaskComment[]; next_cursor: string | null }
export interface TaskGeneration { version: NativeInteger; physical_branch: string | null; inherited_target: string | null }
export interface TaskDetail {
 task: Task; creation: { actor: string; record: Task }; last_event: TaskEvent | null; generation: TaskGeneration;
 history_start_cursor: string; comments_start_cursor: string;
 history: TaskHistoryPage | null; comments: TaskCommentsPage | null
}
export interface TaskAuditPageRequest { page_size: number; cursor?: string }
export interface TaskGetOptions { history?: TaskAuditPageRequest; comments?: TaskAuditPageRequest; signal?: AbortSignal }
export interface TaskSummary extends TaskTimelineItem {
 session_id: string; task_type: string; priority: number; parent_id: string | null; ownership: Ownership;
 progress: ProgressObservation | null; flagged: boolean; ready: boolean; revision: NativeInteger; created_at: NativeInteger; updated_at: NativeInteger
}
export interface CatalogueFilter { query?: string | null; status?: TaskStatus | null; session_id?: string | null; flagged?: boolean | null; blocked?: boolean | null }
/** Opaque native cursor: echo it unchanged only for the same selection and filter. */
export interface CatalogueCursor { root: string; project_version: string; version: NativeInteger; filter: CatalogueFilter; offset: NativeInteger }
export interface TaskCataloguePage { results: TaskSummary[]; next_cursor: CatalogueCursor | null }
export interface TaskCatalogueOptions { selection: TaskSelection; filter?: CatalogueFilter; page_size?: number; cursor?: CatalogueCursor | null; signal?: AbortSignal }

export type TaskSearchMode = 'fts' | 'semantic' | 'hybrid'
/** Native continuation data, echoed unchanged only with the same search. */
export type TaskSearchCursor = Record<string, unknown>
export interface TaskSearchFilter { status?: TaskStatus; session_id?: string; flagged?: boolean; blocked?: boolean }
export interface TaskSearchOptions { selection: TaskSelection; text: string; mode?: TaskSearchMode; filter?: TaskSearchFilter; page_size?: number; top_k?: number; cursor?: TaskSearchCursor | null; signal?: AbortSignal }
export interface TaskSearchHit { task: TaskSummary; score: number; location: { Specification: { revision: NativeInteger } } | { Comment: { comment_id: string; sequence: NativeInteger } } }
export type TaskSearchRead = 'NoCorpus' | { PublishedEmpty: { generation: TaskGeneration } } | { Published: { generation: TaskGeneration; total_hits: number; results: TaskSearchHit[]; next_cursor: TaskSearchCursor | null } }
export type TaskSearchMatches = Record<string, Pick<TaskSearchHit, 'location' | 'score'>>
export interface TaskSearchPage { results: TaskSummary[]; matches: TaskSearchMatches; next_cursor: TaskSearchCursor | null; total_hits: number }

export class TaskReadError extends Error {
 constructor(public readonly status: number, message?: string) { super(message ?? 'Task request failed (HTTP ' + status + ')') }
}
export interface TaskContexts { contexts: TaskContext[]; current: TaskContext | null; projects?: TaskProjectMetadata[] }
export async function taskRead<T>(operation: 'contexts' | 'context' | 'catalogue' | 'get' | 'session' | 'session-heads' | 'search', selection: TaskSelection, input: unknown, signal?: AbortSignal): Promise<T> {
 const base = getApiBase().replace(/\/$/, '')
 const response = await fetch(base + (base.endsWith('/api') ? '' : '/api') + '/tasks/' + operation, {
  method: 'POST', credentials: 'same-origin', signal,
  headers: { 'Content-Type': 'application/json', 'X-Graphit-Request': 'ui' },
  body: stringifyNativeJson({ selection, ...(input === undefined ? {} : { input }) }),
 })
 if (!response.ok) {
  let message: string | undefined
  try {
   const failure = parseNativeJson<{ error?: { code?: string } }>(await response.text())
   if (failure.error?.code === 'model_disabled') message = 'Embedding is disabled for this project. Use Text search or enable embedding.'
   else if (failure.error?.code === 'model_unavailable' || failure.error?.code === 'embedding_service_unavailable') message = 'The embedding service is unavailable. Use Text search or check the configured service.'
   else if (failure.error?.code === 'embedding_service_failure' || failure.error?.code === 'invalid_embedding_response') message = 'The embedding service failed to return a valid result. Retry or check the configured service.'
  } catch { /* Preserve the HTTP failure even when its body is not a native error. */ }
  throw new TaskReadError(response.status, message)
 }
 return parseNativeJson<T>(await response.text())
}
export const taskApi = {
 contexts: (signal?: AbortSignal) => taskRead<TaskContexts>('contexts', {}, undefined, signal),
 context: (selection: TaskSelection = {}, signal?: AbortSignal) => taskRead<TaskContext>('context', selection, undefined, signal),
 catalogue: ({ selection, filter = {}, page_size = 20, cursor = null, signal }: TaskCatalogueOptions) =>
  taskRead<TaskCataloguePage>('catalogue', selection, { filter, page_size, cursor }, signal),
 search: async ({ selection, text, mode = 'fts', filter = {}, page_size = 20, top_k = 100, cursor = null, signal }: TaskSearchOptions): Promise<TaskSearchPage> => {
  const result = await taskRead<TaskSearchRead>('search', selection, { mode, text, filter, page_size, top_k, cursor }, signal)
  if (result === 'NoCorpus' || 'PublishedEmpty' in result) return { results: [], matches: {}, next_cursor: null, total_hits: 0 }
  if (!('Published' in result)) throw new Error('Invalid Task search response')
  return { results: result.Published.results.map(hit => hit.task), matches: Object.fromEntries(result.Published.results.map(hit => [hit.task.id, { location: hit.location, score: hit.score }])), next_cursor: result.Published.next_cursor, total_hits: result.Published.total_hits }
 },
 get: (selection: TaskSelection, id: string, { signal, ...sections }: TaskGetOptions = {}) => taskRead<TaskDetail | null>('get', selection, { id, ...sections }, signal),
}
