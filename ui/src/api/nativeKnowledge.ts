import type { KnowledgeReferenceSource } from './nativeRecordReferences'
import type { NativeInteger } from './nativeJson'
import { nativeRecordRead, nativeRecordResponse } from './nativeRecordRead'
import { readAgentStream } from './agentStream'
import { importedSourceKey, nativeSelectionKey } from './nativeRecordNavigation'
import type { TaskContext } from './task'

export interface NativeKnowledgeGeneration { corpus_uri: string; version: NativeInteger; physical_branch: string | null; inherited_target: string | null }
export interface NativeKnowledgeHead { slug: string; title: string; updated_at: string | null }
export interface NativeKnowledgeDocument {
  slug: string; title: string; body: string; kind: string; summary: string; description: string | null; summary_truncated: boolean
  source_path: string; physical_digest: number[] | null; compilation_digest: number[]; recipe: string
  recipe_parameters: { fallback_title: string; kind_on_absent: string; max_summary_bytes: NativeInteger }
  updated_at: string | null; stale_since: string | null; stale_after: { seconds: NativeInteger; nanoseconds: number } | null
  resolved_wiki_targets: string[]
}
export type NativeKnowledgeHeads = { state: 'no_corpus' } | { state: 'published'; generation: NativeKnowledgeGeneration; heads: NativeKnowledgeHead[] }
export type NativeKnowledgePage = { state: 'no_corpus' } | { state: 'published'; generation: NativeKnowledgeGeneration; page: { state: 'published_empty' } | { state: 'page_absent' } | { state: 'found'; document: NativeKnowledgeDocument } }
export interface KnowledgeEnvelope<T> { selection: TaskContext; source: { kind: 'project_knowledge' }; focused_source: KnowledgeReferenceSource | null; native: T }
export interface ImportedKnowledgeSource { kind: 'imported_knowledge'; artifact: { id: string; version: string; context_alias?: string } }
export interface ImportedKnowledgeEnvelope { source: ImportedKnowledgeSource; focused_source: KnowledgeReferenceSource | null; native: { state: 'registry_absent' } | Extract<NativeKnowledgePage, { state: 'published' }> }
export interface ImportedKnowledgeHeadsEnvelope { source: ImportedKnowledgeSource; native: { state: 'registry_absent' } | Extract<NativeKnowledgeHeads, { state: 'published' }> }

export type NativeKnowledgeSearchMode = 'fts' | 'semantic' | 'hybrid'
export type NativeKnowledgeSearchMetric = 'cosine' | 'l2' | 'dot'
export interface NativeKnowledgeSearchInput {
  query: string; mode: NativeKnowledgeSearchMode; metric?: NativeKnowledgeSearchMetric
  top_k?: NativeInteger; page_size?: number; preview?: boolean; cursor?: unknown; rerank?: boolean
}
export interface NativeKnowledgeSearchHit {
  focused_source: KnowledgeReferenceSource
  lexical_score?: number; semantic_distance?: number; rrf_score?: number; rerank_score?: number
  preview?: { text: string; source: 'summary' | 'body'; range: { start: NativeInteger; end: NativeInteger }; truncated: boolean }
}
export type NativeKnowledgeSearch = { state: 'no_corpus' | 'registry_absent' } | {
  state: 'published_empty' | 'no_hits' | 'hits' | 'partial' | 'unavailable'
  generation: NativeKnowledgeGeneration
  semantic_generation?: Omit<NativeKnowledgeGeneration, 'corpus_uri'> | null
  reason?: string | null; total: NativeInteger; hits: NativeKnowledgeSearchHit[]; next_cursor: unknown | null
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
