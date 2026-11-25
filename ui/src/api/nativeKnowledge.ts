import type { KnowledgeReferenceSource } from './nativeRecordReferences'
import type { NativeInteger } from './nativeJson'
import { nativeRecordRead, nativeRecordResponse } from './nativeRecordRead'
import { readAgentStream } from './agentStream'
import { importedSourceKey, nativeSelectionKey } from './nativeRecordNavigation'
import type { TaskContext } from './task'

export interface NativeKnowledgeGeneration { version: NativeInteger }
export interface NativeKnowledgeHead { slug: string; title: string; updated_at: string | null; source_path?: string }
export interface NativeKnowledgePageLink { target: string; slug: string | null; title: string | null }
export interface NativeKnowledgeDocument {
  slug: string; title: string; body: string
  source_path?: string
  resolved_wiki_targets: string[]
  previous?: NativeKnowledgePageLink | null
  next?: NativeKnowledgePageLink | null
}
export type NativeKnowledgeHeads = { state: 'no_corpus' } | { state: 'published'; heads: NativeKnowledgeHead[] }
export type NativeKnowledgePage = { state: 'no_corpus' } | { state: 'published'; page: { state: 'published_empty' } | { state: 'page_absent' } | { state: 'found'; document: NativeKnowledgeDocument } }
export interface KnowledgeEnvelope<T> { selection: TaskContext; source: { kind: 'project_knowledge' }; focused_source: Pick<KnowledgeReferenceSource, 'slug' | 'namespace'> | null; native: T }
export interface ImportedKnowledgeSource { kind: 'imported_knowledge'; artifact: { id: string; version: string; context_alias?: string } }
export interface ImportedKnowledgeEnvelope { source: ImportedKnowledgeSource; focused_source: Pick<KnowledgeReferenceSource, 'slug' | 'namespace'> | null; native: { state: 'registry_absent' } | Extract<NativeKnowledgePage, { state: 'published' }> }
export interface ImportedKnowledgeHeadsEnvelope { source: ImportedKnowledgeSource; native: { state: 'registry_absent' } | Extract<NativeKnowledgeHeads, { state: 'published' }> }

export type NativeKnowledgeSearchMode = 'fts' | 'semantic' | 'hybrid'
export type NativeKnowledgeSearchMetric = 'cosine' | 'l2' | 'dot'
export interface NativeKnowledgeSearchInput {
  query: string; mode: NativeKnowledgeSearchMode; metric?: NativeKnowledgeSearchMetric
  top_k?: NativeInteger; page_size?: number; preview?: boolean; cursor?: unknown; rerank?: boolean
}
export interface NativeKnowledgeSearchHit {
  slug: string
  title: string
  preview?: { text: string; truncated: boolean }
}
export type NativeKnowledgeSearch = { state: 'no_corpus' | 'registry_absent' } | {
  state: 'published_empty' | 'no_hits' | 'hits' | 'partial' | 'unavailable'
  reason?: string; total: NativeInteger; hits: NativeKnowledgeSearchHit[]; next_cursor: unknown | null
}
export interface ImportedKnowledgeSearchEnvelope {
  source: ImportedKnowledgeSource; focused_source: KnowledgeReferenceSource | null; native: NativeKnowledgeSearch
}

export type NativeKnowledgeAskInput = { query: string; model: string } & (
  { selected_slugs: string[]; retrieval?: never } |
  { selected_slugs?: never; retrieval: { top_k: NativeInteger } & ({ mode: 'fts'; metric?: never } | { mode: 'semantic' | 'hybrid'; metric?: NativeKnowledgeSearchMetric }) }
)
export type NativeKnowledgeAskProgress = Record<string, unknown>
export interface NativeKnowledgeAskResult {
  status: string; generation?: NativeKnowledgeGeneration; provided_slugs?: string[]
  receipt?: { completion?: { status?: string; answer?: string; requested_model?: string; [key: string]: unknown }; [key: string]: unknown }
  retrieval?: Record<string, unknown>; error?: string; category?: string; execution_error?: string | null; delivery_error?: string
  [key: string]: unknown
}

async function readKnowledge<T>(operation: 'heads' | 'page' | 'search', selection: TaskContext, input: unknown, signal?: AbortSignal): Promise<KnowledgeEnvelope<T>> {
  const result = await nativeRecordRead<KnowledgeEnvelope<T>>('/knowledge/' + operation, { selection, input }, signal)
  if (nativeSelectionKey(result.selection) !== nativeSelectionKey(selection)) throw new Error('The Knowledge selection changed during the request')
  return result
}

export const nativeKnowledgeApi = {
  heads: (selection: TaskContext, signal?: AbortSignal) => readKnowledge<NativeKnowledgeHeads>('heads', selection, {}, signal),
  page: (selection: TaskContext, slug: string, signal?: AbortSignal) => readKnowledge<NativeKnowledgePage>('page', selection, { slug }, signal),
  search: (selection: TaskContext, input: NativeKnowledgeSearchInput, signal?: AbortSignal) => readKnowledge<NativeKnowledgeSearch>('search', selection, input, signal),
  ask: async (selection: TaskContext, input: NativeKnowledgeAskInput, options: { signal?: AbortSignal; onProgress?: (event: NativeKnowledgeAskProgress) => void } = {}) => {
    const response = await nativeRecordResponse('/knowledge/ask', { selection, input }, options.signal)
    return readAgentStream<NativeKnowledgeAskResult, NativeKnowledgeAskProgress>(response, options.onProgress)
  },
  importedContexts: (signal?: AbortSignal) => nativeRecordRead<{ contexts: ImportedKnowledgeSource[] }>('/knowledge/imported/contexts', { input: {} }, signal),
  importedHeads: async (source: ImportedKnowledgeSource, signal?: AbortSignal): Promise<ImportedKnowledgeHeadsEnvelope> => {
    const result = await nativeRecordRead<ImportedKnowledgeHeadsEnvelope>('/knowledge/imported/heads', { input: { source } }, signal)
    if (importedSourceKey(result.source) !== importedSourceKey(source)) throw new Error('The imported Knowledge selection changed during the request')
    return result
  },
  importedPage: async (source: ImportedKnowledgeSource, slug: string, signal?: AbortSignal): Promise<ImportedKnowledgeEnvelope> => {
    const result = await nativeRecordRead<ImportedKnowledgeEnvelope>('/knowledge/imported/page', { input: { source, slug } }, signal)
    if (importedSourceKey(result.source) !== importedSourceKey(source)) throw new Error('The imported Knowledge selection changed during the request')
    return result
  },
  importedSearch: async (source: ImportedKnowledgeSource, search: NativeKnowledgeSearchInput, signal?: AbortSignal): Promise<ImportedKnowledgeSearchEnvelope> => {
    const result = await nativeRecordRead<ImportedKnowledgeSearchEnvelope>('/knowledge/imported/search', { input: { source, search } }, signal)
    if (importedSourceKey(result.source) !== importedSourceKey(source)) throw new Error('The imported Knowledge selection changed during the request')
    return result
  },
}
