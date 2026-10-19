import { beforeEach, expect, it, vi } from 'vitest'
import { nativeRecordRead } from './nativeRecordRead'
import { nativeRecordReferencesApi, knowledgeReferenceBinding } from './nativeRecordReferences'
import { importedReferenceSource, referenceBinding, referenceSelection, referenceSource, referencesReply } from './nativeRecordReferences.test-fixtures'
vi.mock('./nativeRecordRead', () => ({ nativeRecordRead: vi.fn() }))
beforeEach(() => vi.resetAllMocks())
it('uses the existing native POST codec with the literal physical binding and abort signal', async () => {
 const response = referencesReply(), abort = new AbortController()
 vi.mocked(nativeRecordRead).mockResolvedValue(response)
 const request = { selection: referenceSelection, input: { focus: referenceBinding.focus, outgoing_source: referenceBinding.outgoing_source, include_user: false } }
 expect(await nativeRecordReferencesApi.read(request, abort.signal)).toBe(response)
 expect(nativeRecordRead).toHaveBeenCalledWith('/records/references', request, abort.signal)
})
it('rejects a changed physical path even when id and version match', async () => {
 vi.mocked(nativeRecordRead).mockResolvedValue({ ...referencesReply(), selection: { ...referenceSelection, project_dir: "/p" } })
 await expect(nativeRecordReferencesApi.read({ selection: referenceSelection, input: { focus: referenceBinding.focus } })).rejects.toThrow('selection changed')
})
it('omits PROJECT selection in an imported-only request and preserves an opaque cursor intact', async () => {
 // This transport fixture is opaque by design; server cursor validation is qualified separately.
 const cursor = { literal: 9007199254740993n, inner: null }, reply = { ...referencesReply(), selection: null }
 vi.mocked(nativeRecordRead).mockResolvedValue(reply)
 const request = { input: { focus: referenceBinding.focus, outgoing_source: importedReferenceSource, pages: [{ source: importedReferenceSource, direction: 'incoming' as const, cursor }] } }
 expect(await nativeRecordReferencesApi.read(request)).toBe(reply)
 expect(nativeRecordRead).toHaveBeenCalledWith('/records/references', request, undefined)
 expect(Object.prototype.hasOwnProperty.call(request, 'selection')).toBe(false)
})
it('takes Page namespace and witness without deriving them from producer or physical selection', () => {
 const focused = { ...referenceSource, namespace: { scope: { project: 'namespace B' }, context: 'literal context' } }
 const binding = knowledgeReferenceBinding(null, importedReferenceSource, focused)!
 expect(binding.focus).toEqual({ kind: 'knowledge', scope: { project: 'namespace B' }, id: 'Page', context: 'literal context' })
 expect(binding.outgoing_source).toBe(importedReferenceSource)
 expect(knowledgeReferenceBinding(referenceSelection, { kind: 'project_knowledge' }, { ...focused, witness: null })).toBeNull()
 expect(knowledgeReferenceBinding(referenceSelection, { kind: 'project_knowledge' }, null)).toBeNull()
 expect(Object.prototype.hasOwnProperty.call(knowledgeReferenceBinding(referenceSelection, { kind: 'project_knowledge' }, referenceSource)!.focus, 'context')).toBe(false)
})
