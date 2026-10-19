import { expect, it } from 'vitest'
import { nativeReferencesView } from './nativeReferencesView'
import { knowledgeReferences, referenceSelection } from '@/api/nativeRecordReferences.test-fixtures'
import type { ReferenceSourceResult } from '@/api/nativeRecordReferences'
it('preserves native payloads, empty authored fields, literal ordinals and page integrity separately from EOF', () => {
 const source = knowledgeReferences(), view = nativeReferencesView(source)
 expect(view.raw).toBe(source)
 expect(view.outgoing?.edges[0].field).toBe('')
 expect(view.outgoing?.edges[0].targetTitle).toBe('')
 expect(view.outgoing?.native).toBe(typeof source.native === 'object' && 'published' in source.native ? source.native.published && 'read' in source.native.published ? source.native.published.read.outgoing : null : null)
})
it('normalizes only the Memory SDK scope projection while retaining the CamelCase native reference', () => {
 const raw = { source: 'source-memory', source_revision: 9007199254740993n, reference_generation: 1, source_snapshot: null, reference: { target_kind: 'knowledge' as const, scope: { Project: 'namespace B' }, id: 'Target', context: '', relation: 'supports', field: null }, provenance: 'authored' as const, ordinal: 0, source_observation: 'unresolved' as const, target_resolution: 'unresolved' as const }
 const source: ReferenceSourceResult = { source: { kind: 'project_memory' }, native: { published: { generation: { sdk_version: 1, project_id: referenceSelection.project_id, project_version: referenceSelection.project_version, physical_branch: null, inherited_target: null }, outgoing: { results: [raw], total: 1, complete: true, next_cursor: null }, incoming: null, coverage: { project_id: referenceSelection.project_id, memory_project_source_complete: true, inbound_other_sources_not_qualified: true }, warnings: [] } }, navigation: { outgoing: [{ source_href: null, source_reader: null, target_rich: null }], incoming: [] } }
 const view = nativeReferencesView(source)
 expect(view.outgoing?.edges[0].target).toEqual({ kind: 'knowledge', scope: { project: 'namespace B' }, id: 'Target', context: '' })
 expect(view.outgoing?.edges[0].raw).toBe(raw)
 expect(raw.reference.scope).toEqual({ Project: 'namespace B' })
 expect(view.incoming).toBeNull()
})
it('keeps absent corpus and absent artifact distinct and refuses mismatched navigation', () => {
 expect(nativeReferencesView({ source: { kind: 'project_task' }, native: 'no_corpus', navigation: { outgoing: [], incoming: [] } }).state).toBe('no_corpus')
 const source = knowledgeReferences()
 expect(() => nativeReferencesView({ ...source, navigation: { outgoing: [], incoming: [] } })).toThrow('navigation')
})
