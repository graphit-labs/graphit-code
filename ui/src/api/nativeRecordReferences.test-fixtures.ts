import type { ImportedKnowledgeSource } from './nativeKnowledge'
import type { KnowledgeReferenceEdge, KnowledgeReferenceSource, NativeReferences, ReferenceBinding, ReferenceNavigation, ReferenceReader, ReferenceSourceResult } from './nativeRecordReferences'
import type { TaskContext } from './task'
export const referenceSelection: TaskContext = { project_dir: "/ÿp", project_id: 'observed-project', project_version: ' literal/version ' }
export const referenceGeneration = { corpus_uri: 'project-knowledge', version: 9007199254740993n, physical_branch: null, inherited_target: null }
export const referenceSource: KnowledgeReferenceSource = { slug: 'Page', title: '', source_path: 'docs/page.md', compilation_digest: Array(32).fill(1), namespace: { scope: { project: 'observed-namespace' }, context: null }, witness: Array(32).fill(2) }
export const referenceBinding: ReferenceBinding = { selection: referenceSelection, outgoing_source: { kind: 'project_knowledge' }, focus: { kind: 'knowledge', scope: { project: 'observed-namespace' }, id: 'Page' } }
export const importedReferenceSource: ImportedKnowledgeSource = { kind: 'imported_knowledge', artifact: { id: 'observed-artifact', version: ' literal/pin ', context_alias: 'namespace B' } }
export function knowledgeReader(id: string, scope = 'observed-namespace'): ReferenceReader {
 return { selection: referenceSelection, input: { slug: id }, availability: 'available', method: 'POST', path: '/api/knowledge/page', focus: { kind: 'knowledge', scope: { project: scope }, id }, source: { kind: 'project_knowledge' } }
}
export function knowledgeEdge(id = 'Target', ordinal: number | bigint = 0): KnowledgeReferenceEdge {
 return { source: referenceSource, payload: { authored: { target: { target_kind: 'knowledge', namespace: { scope: { project: 'observed-namespace' }, context: null }, id }, relation: 'supports', field: '' } }, provenance: 'authored', ordinal, target_observation: 'unresolved' }
}
export function knowledgeNavigation(id = 'Target'): ReferenceNavigation {
 return { source_href: null, source_reader: knowledgeReader('Page'), target_rich: { title: '', reader: knowledgeReader(id), href: null, generation: referenceGeneration } }
}
export function knowledgeReferences(edges: KnowledgeReferenceEdge[] = [knowledgeEdge()], cursor: unknown | null = null): ReferenceSourceResult {
 return { source: { kind: 'project_knowledge' }, native: { published: { generation: referenceGeneration, read: { focused_source: referenceSource, outgoing: { results: edges, total: 9007199254740993n, complete: true, next_cursor: cursor }, incoming: { results: [], total: 0, complete: true, next_cursor: null }, coverage: { knowledge_source_complete: true, inbound_other_sources_not_qualified: true }, warnings: [] } } }, navigation: { outgoing: edges.map(edge => knowledgeNavigation('authored' in edge.payload ? edge.payload.authored.target.id : edge.payload.wiki_link.target.id)), incoming: [] } }
}
export function referencesReply(source = knowledgeReferences()): NativeReferences {
 return { selection: referenceSelection, sources: [
  { source: { kind: 'project_task' }, native: 'no_corpus', navigation: { outgoing: [], incoming: [] } },
  { source: { kind: 'project_memory' }, native: 'no_corpus', navigation: { outgoing: [], incoming: [] } }, source,
 ], outgoing_complete: true, incoming_complete: true, warnings: [] }
}
