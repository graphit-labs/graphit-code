import type { NativeInteger } from './nativeJson'
import { nativeRecordRead } from './nativeRecordRead'
import { nativeSelectionKey } from './nativeRecordNavigation'
import type { TaskContext } from './task'

export type MemoryClassification = 'fact' | 'skill' | 'tension' | 'decision' | 'convention' | 'correction'
export interface NativeMemory {
  id: string; revision: NativeInteger; title: string; body: string
  attributes: { classification: MemoryClassification; mandatory: boolean; important: boolean }
  tags: string[]; references: MemoryReference[]; reference_generation: NativeInteger
  created_by: string; updated_by: string; created_at: NativeInteger; updated_at: NativeInteger
}
export interface MemoryReference {
  target_kind: string; scope: { Project: string } | { User: string }; id: string
  context: string | null; relation: string; field: string | null
}
export interface UserMemoryOwner { installation_id: string; identity: { issuer: string; subject: string } }
export interface ProjectMemoryGeneration { sdk_version: NativeInteger; project_id: string; project_version: string; physical_branch: string | null; inherited_target: string | null }
export interface UserMemoryGeneration { owner: UserMemoryOwner; sdk_version: NativeInteger; physical_branch: string | null; inherited_target: string | null }
export type MemoryGeneration = ProjectMemoryGeneration | UserMemoryGeneration
export interface MemoryCreation {
  project_id?: string; operation_version?: string; owner?: UserMemoryOwner; actor: string
  input: { idempotency_key: string; title: string; body: string; attributes: NativeMemory['attributes']; tags: string[]; references: MemoryReference[] | null }
  memory: NativeMemory
}
export interface MemoryRevision {
  project_id?: string; operation_version?: string; owner?: UserMemoryOwner; actor: string
  operation: unknown; before: NativeMemory; after: NativeMemory
}
export interface NativeMemoryHistory { creation: MemoryCreation; current: NativeMemory; deleted: boolean; revisions: MemoryRevision[]; generation: MemoryGeneration }
export interface NativeMemoryDetail { memory: NativeMemory; creation: MemoryCreation; generation: MemoryGeneration }
export type NativeMemoryList = { state: 'no_corpus' } | { state: 'published'; generation: MemoryGeneration; memories: NativeMemory[] }
export type MemoryEnvelope<T> = { selection: TaskContext; native: T } | { scope_id: string; currentOwner: UserMemoryOwner; native: T }
export interface NativeMemoryFilter { classification?: MemoryClassification; tag?: string; mandatory?: boolean; important?: boolean; exclude_mandatory?: boolean }
export type MemoryReadScope = { kind: 'project'; selection: TaskContext } | { kind: 'user'; requested_scope_id?: string }
export type MemorySearchMode = 'fts' | 'semantic' | 'hybrid'
export type MemoryMatchLocation = { kind: 'current' } | { kind: 'archived'; revision: NativeInteger }
export type MemorySearchHit = { current: NativeMemory; location: MemoryMatchLocation; matched: NativeMemory; deleted: boolean; score: number }
  | { current: NativeMemory; location: MemoryMatchLocation; matched_revision: NativeInteger; distance: number }
  | { current: NativeMemory; lexical: { matched_revision: NativeInteger; location: MemoryMatchLocation; score: number } | null; semantic: { matched_revision: NativeInteger; location: MemoryMatchLocation; distance: number } | null; score: number }
export type NativeMemorySearch = { mode: MemorySearchMode; model: { vector_id: number[]; dimensions: NativeInteger; projection_recipe: string } | null } & (
  { state: 'no_corpus' } | { state: 'published_empty'; generation: MemoryGeneration } |
  { state: 'published'; generation: MemoryGeneration; total_hits: NativeInteger; hits: MemorySearchHit[] } |
  { state: 'unavailable'; reason: 'model_unavailable' | 'missing_coverage' | 'incompatible_identity' }
)
export interface MemorySearchInput { text: string; top_k: number; mode?: MemorySearchMode; filter?: NativeMemoryFilter; include_deleted_history?: boolean }

async function readMemory<T>(operation: 'list' | 'get' | 'history' | 'search', scope: MemoryReadScope, input: unknown, signal?: AbortSignal): Promise<MemoryEnvelope<T>> {
  const body = scope.kind === 'project' ? { selection: scope.selection, input } : { input }
  const result = await nativeRecordRead<MemoryEnvelope<T>>('/' + (scope.kind === 'project' ? 'memory' : 'user-memory') + '/' + operation, body, signal)
  if (scope.kind === 'project') {
    if (!('selection' in result) || nativeSelectionKey(result.selection) !== nativeSelectionKey(scope.selection)) throw new Error('The Memory selection changed during the request')
  } else if (!('scope_id' in result) || (scope.requested_scope_id !== undefined && result.scope_id !== scope.requested_scope_id)) {
    throw new Error('The selected User Memory namespace is unavailable')
  }
  return result
}

export const nativeMemoryApi = {
  list: (scope: MemoryReadScope, filter: NativeMemoryFilter, signal?: AbortSignal) => readMemory<NativeMemoryList>('list', scope, filter, signal),
  get: (scope: MemoryReadScope, id: string, signal?: AbortSignal) => readMemory<NativeMemoryDetail | null>('get', scope, { id }, signal),
  history: (scope: MemoryReadScope, id: string, signal?: AbortSignal) => readMemory<NativeMemoryHistory | null>('history', scope, { id }, signal),
  search: (scope: MemoryReadScope, input: MemorySearchInput, signal?: AbortSignal) => readMemory<NativeMemorySearch>('search', scope, input, signal),
}
