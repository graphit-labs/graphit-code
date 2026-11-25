import { expect, it } from 'vitest'
import { referenceReaderHref, requestedReference, referenceBindingMatches } from './nativeReferenceNavigation'
import type { ReferenceReader } from './nativeRecordReferences'
import { importedReferenceSource, knowledgeReader, referenceBinding, referenceSelection } from './nativeRecordReferences.test-fixtures'
it('maps an available reader with null server href to a known native route and preserves literal selection/focus', () => {
 const href = referenceReaderHref(knowledgeReader('literal/%2F page'))!
 const url = new URL(href, 'http://test.invalid')
 expect(url.pathname).toBe('/knowledge/explorer')
 expect(url.searchParams.get('page')).toBe('literal/%2F page')
 expect(JSON.parse(url.searchParams.get('selection')!)).toEqual({ project_id: referenceSelection.project_id, project_version: referenceSelection.project_version })
 expect(requestedReference(url.search)?.focus.id).toBe('literal/%2F page')
})
it('keeps an imported producer separate from its observed namespace in the reload URI', () => {
 const reader: ReferenceReader = { ...knowledgeReader('Page', 'namespace B'), selection: null, source: importedReferenceSource, input: { source: importedReferenceSource, slug: 'Page' }, path: '/api/knowledge/imported/page' }
 const url = new URL(referenceReaderHref(reader)!, 'http://test.invalid')
 expect(url.searchParams.has('selection')).toBe(false)
 expect(requestedReference(url.search)).toEqual({ focus: reader.focus, source: importedReferenceSource })
 expect(referenceBindingMatches(requestedReference(url.search), { ...referenceBinding, selection: null, focus: reader.focus, outgoing_source: importedReferenceSource })).toBe(true)
 expect(referenceBindingMatches(requestedReference(url.search), referenceBinding)).toBe(false)
})
it('maps Task, Session and PROJECT/USER Memory through their own native reader routes', () => {
 const cases: Array<{ reader: ReferenceReader; route: string }> = [
  ...(['task', 'session', 'memory'] as const).map(kind => ({
   reader: { selection: referenceSelection, availability: 'available' as const, method: 'POST' as const,
    path: kind === 'memory' ? '/api/memory/get' : kind === 'task' ? '/api/tasks/get' : '/api/tasks/session',
    input: { id: ' literal/%2F id ' }, focus: { kind, scope: { project: referenceSelection.project_id }, id: ' literal/%2F id ' },
    source: { kind: kind === 'memory' ? 'project_memory' as const : 'project_task' as const } },
   route: kind === 'memory' ? '/memory/explorer/project' : kind === 'task' ? '/task/explorer' : '/task/sessions',
  })),
  { reader: { selection: null, availability: 'available', method: 'POST', path: '/api/user-memory/get', input: { id: ' literal/%2F id ' },
    focus: { kind: 'memory', scope: { user: ' observed own scope ' }, id: ' literal/%2F id ' }, source: { kind: 'user_memory' } }, route: '/memory/explorer/user' },
 ]
 for (const { reader, route } of cases) {
  const href = referenceReaderHref(reader)
  expect(href).not.toBeNull()
  const url = new URL(href!, 'http://test.invalid')
  expect(url.pathname).toBe(route + '/' + encodeURIComponent(reader.focus.id))
  expect(requestedReference(url.search)).toEqual({ focus: reader.focus, source: reader.source })
  if (reader.selection) expect(JSON.parse(url.searchParams.get('selection')!)).toEqual({ project_id: referenceSelection.project_id, project_version: referenceSelection.project_version })
  else {
   expect(url.searchParams.has('selection')).toBe(false)
   expect(url.searchParams.get('scope_id')).toBe(' observed own scope ')
  }
 }
})
it('rejects physical and logical mismatches in PROJECT and USER reader locators', () => {
 const reader: ReferenceReader = { selection: referenceSelection, availability: 'available', method: 'POST', path: '/api/memory/get', input: { id: 'm' },
  focus: { kind: 'memory', scope: { project: referenceSelection.project_id }, id: 'm' }, source: { kind: 'project_memory' } }
 expect(referenceReaderHref({ ...reader, focus: { ...reader.focus, scope: { project: 'other' } } })).toBeNull()
 expect(referenceReaderHref({ ...reader, source: { kind: 'project_task' } })).toBeNull()
 expect(referenceReaderHref({ ...reader, path: '/api/user-memory/get', source: { kind: 'user_memory' }, focus: { ...reader.focus, scope: { user: 'own' } } })).toBeNull()
 expect(referenceReaderHref({ ...reader, selection: null })).toBeNull()
})
it('leaves a missing reader, unknown endpoint and incoherent id noninteractive', () => {
 expect(referenceReaderHref(null)).toBeNull()
 expect(referenceReaderHref({ ...knowledgeReader('Page'), path: '/api/arbitrary' })).toBeNull()
 expect(referenceReaderHref({ ...knowledgeReader('Page'), input: { slug: 'Other' } })).toBeNull()
})
it('rejects malformed imported input tuples without throwing from render', () => {
 const reader = { ...knowledgeReader('Page'), selection: null, source: importedReferenceSource, path: '/api/knowledge/imported/page' }
 for (const source of [null, { kind: 'imported_knowledge' }, { ...importedReferenceSource, artifact: { ...importedReferenceSource.artifact, version: 'other' } }]) {
  expect(() => referenceReaderHref({ ...reader, input: { source, slug: 'Page' } } as unknown as ReferenceReader)).not.toThrow()
  expect(referenceReaderHref({ ...reader, input: { source, slug: 'Page' } } as unknown as ReferenceReader)).toBeNull()
 }
})
it('rejects ambiguous, present-null or nonliteral reference URI fields', () => {
 expect(() => requestedReference('?reference_focus=null&reference_source=null')).toThrow('focus')
 expect(() => requestedReference('?reference_focus={}&reference_focus={}')).toThrow('ambiguous')
 const params = new URLSearchParams({ reference_focus: JSON.stringify({ ...referenceBinding.focus, kind: ['knowledge'] }), reference_source: '{"kind":"project_knowledge"}' })
 expect(() => requestedReference('?' + params)).toThrow('focus')
})
