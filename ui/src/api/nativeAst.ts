import { nativeRecordRead } from './nativeRecordRead'
import type { TaskContext } from './task'
import type { NativeInteger } from './nativeJson'

export interface AstSelection { project_dir: TaskContext['project_dir']; project_version: string }
export const astSelection = (context: TaskContext): AstSelection => ({ project_dir: context.project_dir, project_version: context.project_version })
export interface AstContexts { contexts: TaskContext[]; current: TaskContext | null }
export type AstGeneration = number[]
export type AstColumn = [string, string]
export interface AstSchemaNode { label: string; primary_key: string; rows: string; columns: AstColumn[]; language_counts: [string, string][] }
export interface AstSchemaRelationship { kind: string; from_label: string; to_label: string; rows: string; properties: AstColumn[] }
export interface AstSchema { generation: AstGeneration; nodes: AstSchemaNode[]; relationships: AstSchemaRelationship[] }
export interface AstNodeId { table: number; row: string }
export interface AstEdgeId { member: number; row: string }
export interface AstSampleNode { id: AstNodeId; label: string; name: string | null; path: string | null; uid: string | null; cluster: string | null; lang: string | null; line: string | null; primary_key?: { name: string; value: AstKey } }
export interface AstSampleEdge { id: AstEdgeId; from: AstNodeId; to: AstNodeId; relationship: string; uid: string }
export interface AstSample { generation: AstGeneration; nodes: AstSampleNode[]; edges: AstSampleEdge[] }
export interface AstCell { type: string; value: unknown }
export interface AstQuery { columns: string[]; rows: AstCell[][] }
export type AstKey = { type: 'string' | 'int64'; value: string }
export interface AstCursor { generation: AstGeneration; fingerprint: number[]; last_uid: string }
export interface AstProperty { name: string; value: { type: 'string'; value: string } | { type: 'int64'; value: string } | { type: 'bool'; value: boolean } | { type: 'null'; value?: null } }
export interface AstNeighborhoodNode { id: AstNodeId; label: string; properties: AstProperty[] }
export interface AstNeighborhoodEdge { id: AstEdgeId; from: AstNodeId; to: AstNodeId; relationship: string; uid: string; properties: AstProperty[] }
export interface AstNeighborhood { generation: AstGeneration; anchor: AstNodeId; nodes: AstNeighborhoodNode[]; edges: AstNeighborhoodEdge[]; next_cursor: AstCursor | null }
export interface AstNeighborhoodInput { anchor_label: string; anchor_key: AstKey; relationship: string; other_label: string; direction: 'incoming' | 'outgoing'; limit: number; cursor?: AstCursor }
export interface AstSourceInput {
  path: string; entity?: string; entity_type?: string
  head?: NativeInteger; tail?: NativeInteger; start_line?: NativeInteger; end_line?: NativeInteger
  pattern?: string; regex?: boolean; before?: NativeInteger; after?: NativeInteger; line_numbers?: boolean
}
export interface AstSourceMatch { line_number: NativeInteger; line: string; is_match: boolean }
export interface AstSourceSlice { source: string; total_lines: NativeInteger; start_line?: NativeInteger; end_line?: NativeInteger; matches?: AstSourceMatch[] }
export interface AstSourceReply { origin: 'indexed' | 'physical'; selection: AstSourceSlice }

export interface AstImportedSource { id: string; version: string }
export interface AstInstalledContext extends AstImportedSource { context: string; backend: 'native' | 'remote_native' }
export interface AstImports { imports: AstInstalledContext[] }
export type AstSearchMode = 'fts' | 'semantic' | 'vector' | 'hybrid'
export type AstSearchMetric = 'l2' | 'cosine' | 'dot'
export interface AstSearchInput { query: string; mode?: AstSearchMode; k?: number; metric?: AstSearchMetric; rerank?: boolean }
export interface AstSearchEntity {
  entity_key: string; uid: string; data_key: string; name: string; kind: string; path: string
  language: string; grammar: string; source_sha256: string; extent_status: string; raw_version: NativeInteger
  start_byte: NativeInteger | null; end_byte: NativeInteger | null; start_line: NativeInteger | null; end_line: NativeInteger | null
  score?: number; lexical_score?: number | null; distance?: number | null; rerank_score?: number
}
export interface AstSearchReply {
  status: string; mode?: AstSearchMode; results: AstSearchEntity[]; vector_status?: string
  model?: unknown
}

const importedSourceKey = (source: AstImportedSource): AstImportedSource => ({ id: source.id, version: source.version })

export const nativeAstApi = {
  contexts: (signal?: AbortSignal) => nativeRecordRead<AstContexts>('/ast/contexts', {}, signal),
  schema: (selection: AstSelection, signal?: AbortSignal) => nativeRecordRead<AstSchema>('/ast/schema', { selection }, signal),
  sample: (selection: AstSelection, signal?: AbortSignal) => nativeRecordRead<AstSample>('/ast/sample', { selection }, signal),
  query: (selection: AstSelection, query: string, parameters: Record<string, unknown> = {}, signal?: AbortSignal) => nativeRecordRead<AstQuery>('/ast/query', { selection, query: { query, parameters } }, signal),
  neighborhood: (selection: AstSelection, neighborhood: AstNeighborhoodInput, signal?: AbortSignal) => nativeRecordRead<AstNeighborhood>('/ast/neighborhood', { selection, neighborhood }, signal),
  source: (selection: AstSelection, source: AstSourceInput, signal?: AbortSignal) => nativeRecordRead<AstSourceReply>('/ast/source', { selection, source }, signal),
  imports: (signal?: AbortSignal) => nativeRecordRead<AstImports>('/ast/imports', {}, signal),
  search: (selection: AstSelection, search: AstSearchInput, signal?: AbortSignal) => nativeRecordRead<AstSearchReply>('/ast/search', { selection, search }, signal),
  importedSchema: (source: AstImportedSource, signal?: AbortSignal) => nativeRecordRead<AstSchema>('/ast/imported/schema', { source: importedSourceKey(source) }, signal),
  importedSample: (source: AstImportedSource, signal?: AbortSignal) => nativeRecordRead<AstSample>('/ast/imported/sample', { source: importedSourceKey(source) }, signal),
  importedQuery: (source: AstImportedSource, query: string, parameters: Record<string, unknown> = {}, signal?: AbortSignal) => nativeRecordRead<AstQuery>('/ast/imported/query', { source: importedSourceKey(source), query: { query, parameters } }, signal),
  importedNeighborhood: (source: AstImportedSource, neighborhood: AstNeighborhoodInput, signal?: AbortSignal) => nativeRecordRead<AstNeighborhood>('/ast/imported/neighborhood', { source: importedSourceKey(source), neighborhood }, signal),
  importedSource: (source: AstImportedSource, input: AstSourceInput, signal?: AbortSignal) => nativeRecordRead<AstSourceReply>('/ast/imported/source', { source: importedSourceKey(source), input }, signal),
  importedSearch: (source: AstImportedSource, search: AstSearchInput, signal?: AbortSignal) => nativeRecordRead<AstSearchReply>('/ast/imported/search', { source: importedSourceKey(source), search }, signal),
}
