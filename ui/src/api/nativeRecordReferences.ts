import type { TaskContext } from './task'
import type { NativeInteger } from './nativeJson'
import type { ImportedKnowledgeSource } from './nativeKnowledge'
import { nativeRecordRead } from './nativeRecordRead'
import { nativeSelectionKey } from './nativeRecordNavigation'

export type ReferenceScope = { project: string } | { user: string }
export type ReferenceKind = 'task' | 'session' | 'memory' | 'knowledge'
export interface ReferenceFocus { kind: ReferenceKind; scope: ReferenceScope; id: string; context?: string }
export type ReferenceSource = { kind: 'project_task' | 'project_memory' | 'project_knowledge' | 'user_memory' } | ImportedKnowledgeSource
export type ReferenceDirection = 'outgoing' | 'incoming'
export interface ReferencePageRequest { source: ReferenceSource; direction: ReferenceDirection; cursor?: unknown }
export interface ReferenceInput { focus: ReferenceFocus; outgoing_source?: ReferenceSource; page_size?: number; include_user?: boolean; contexts?: ImportedKnowledgeSource['artifact'][]; pages?: ReferencePageRequest[] }
export interface ReferenceRequest { selection?: TaskContext; input: ReferenceInput }
export interface ReferenceBinding { selection: TaskContext | null; focus: ReferenceFocus; outgoing_source: ReferenceSource }
export interface ReferenceReader { selection: TaskContext | null; input: { id: string } | { slug: string } | { source: ImportedKnowledgeSource; slug: string }; availability: 'available'; method: 'POST'; path: string; focus: ReferenceFocus; source: ReferenceSource }
export interface ReferenceNavigation { source_href: string | null; source_reader: ReferenceReader | null; target_rich: { title: string; reader: ReferenceReader; href: string | null; generation: unknown | null } | null }
export interface KnowledgeReferenceSource { slug: string; title: string; source_path: string; compilation_digest: number[]; namespace: { scope: ReferenceScope; context: string | null } | null; witness: number[] | null }
export interface SdkReference { target_kind: ReferenceKind; scope: ReferenceScope; id: string; context: string | null; relation: string; field: string | null }
export type TaskResolution = 'unresolved' | { task: { id: string; title: string; status: string } } | { session: { id: string; title: string; status: string } }
export interface TaskReferenceEdge { source: { task: string } | { session: string }; reference: SdkReference; provenance: 'structural' | 'authored'; ordinal: NativeInteger; source_revision: NativeInteger; reference_generation: NativeInteger; source_snapshot: unknown; source_observation: TaskResolution; target_resolution: TaskResolution }
export interface MemoryReferenceEdge { source: string; source_scope?: string; reference: Omit<SdkReference, 'scope'> & { scope: { Project: string } | { User: string } }; provenance: 'authored'; ordinal: NativeInteger; source_revision: NativeInteger; reference_generation: NativeInteger; source_snapshot: unknown; source_observation: 'unresolved' | { memory: { id: string; title: string } }; target_resolution: 'unresolved' | { memory: { id: string; title: string } } }
export interface KnowledgeReferenceTarget { target_kind: ReferenceKind; namespace: { scope: ReferenceScope; context: string | null }; id: string }
export interface KnowledgeReferenceEdge { source: KnowledgeReferenceSource; payload: { authored: { target: KnowledgeReferenceTarget; relation: string; field: string | null } } | { wiki_link: { target: KnowledgeReferenceTarget } }; provenance: 'authored' | 'wiki_link'; ordinal: NativeInteger; target_observation: 'unresolved' | { knowledge_head: { slug: string; title: string } } }
export type NativeReferenceEdge = TaskReferenceEdge | MemoryReferenceEdge | KnowledgeReferenceEdge
export interface ReferencePage<E = NativeReferenceEdge> { results: E[]; total: NativeInteger; complete: boolean; next_cursor: unknown | null }
export interface ReferenceRead { focused_source?: KnowledgeReferenceSource | null; outgoing?: ReferencePage | null; incoming: ReferencePage | null; coverage: Record<string, unknown>; warnings: unknown[] }
export type NativeReferenceRead = 'no_corpus' | { published: (ReferenceRead & { generation: unknown; focus_revision?: NativeInteger }) | { generation: unknown; read: ReferenceRead } } | { state: 'registry_absent' } | { state: 'published'; producer: unknown; generation: unknown; read: ReferenceRead }
export interface ReferenceSourceResult { source: ReferenceSource; native: NativeReferenceRead; navigation: { outgoing: ReferenceNavigation[]; incoming: ReferenceNavigation[] } }
export interface NativeReferences { selection: TaskContext | null; sources: ReferenceSourceResult[]; outgoing_complete: boolean; incoming_complete: boolean; warnings: string[] }

export const nativeRecordReferencesApi = {
  read: async (request: ReferenceRequest, signal?: AbortSignal): Promise<NativeReferences> => {
    const result = await nativeRecordRead<NativeReferences>('/records/references', request, signal)
    if (nativeSelectionKey(result.selection) !== nativeSelectionKey(request.selection ?? null)) throw new Error('The reference selection changed during the request')
    return result
  },
}

export function knowledgeReferenceBinding(selection: TaskContext | null, source: ReferenceSource, focused: KnowledgeReferenceSource | null): ReferenceBinding | null {
  if ((source.kind !== 'project_knowledge' && source.kind !== 'imported_knowledge') || (source.kind === 'project_knowledge' && !selection) || !focused?.namespace || !focused.witness) return null
  return { selection, outgoing_source: source, focus: { kind: 'knowledge', scope: focused.namespace.scope, id: focused.slug, ...(focused.namespace.context === null ? {} : { context: focused.namespace.context }) } }
}

export function projectReferenceBinding(selection: TaskContext, kind: 'task' | 'session' | 'memory', id: string): ReferenceBinding {
  return { selection, focus: { kind, scope: { project: selection.project_id }, id }, outgoing_source: { kind: kind === 'memory' ? 'project_memory' : 'project_task' } }
}
export function userMemoryReferenceBinding(observedScope: string, id: string): ReferenceBinding {
  return { selection: null, focus: { kind: 'memory', scope: { user: observedScope }, id }, outgoing_source: { kind: 'user_memory' } }
}
