import '@testing-library/jest-dom/vitest'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { nativeRecordReferencesApi, type NativeReferences } from '@/api/nativeRecordReferences'
import { NativeRecordReadError } from '@/api/nativeRecordRead'
import { referenceBinding, referenceSelection, knowledgeReferences, knowledgeEdge, referencesReply } from '@/api/nativeRecordReferences.test-fixtures'
import { RecordReferences } from './RecordReferences'
vi.mock('@/api/nativeRecordReferences', async original => ({ ...await original<typeof import('@/api/nativeRecordReferences')>(), nativeRecordReferencesApi: { read: vi.fn() } }))
beforeEach(() => { vi.resetAllMocks(); vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue(referencesReply()) })
afterEach(cleanup)
const wrapper = ({ children }: { children: React.ReactNode }) => <MemoryRouter>{children}</MemoryRouter>
it('renders observed Reader locators as native links, preserving literal scope and fields', async () => {
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 const link = await screen.findByRole('link', { name: /Target/ })
 expect(link.getAttribute('href')).toContain('/knowledge/explorer?')
 expect(link.getAttribute('href')).toContain('reference_focus=')
 expect(screen.getByText(/field ""/)).toBeTruthy()
 expect(nativeRecordReferencesApi.read).toHaveBeenCalledWith(expect.objectContaining({ selection: referenceSelection, input: expect.objectContaining({ include_user: false, focus: referenceBinding.focus }) }), expect.any(AbortSignal))
})
it('distinguishes absent corpus from published empty relationships', async () => {
 const source = knowledgeReferences([])
 if (typeof source.native === 'object' && 'published' in source.native && 'read' in source.native.published) source.native.published.read.outgoing!.total = 0
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue(referencesReply(source))
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 expect(await screen.findAllByText('No links in this requested source.')).toHaveLength(2)
 expect(screen.queryByText('This source has no published corpus.')).toBeNull()
 await user.click(screen.getByRole('checkbox', { name: 'Include other project sources' }))
 await waitFor(() => expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(2))
 await screen.findAllByText('This source has no published corpus.')
 expect(screen.getAllByText('This source has no published corpus.')).toHaveLength(2)
 expect(screen.queryByText('No linked records.')).toBeNull()
})
it('marks a requested null projection unavailable and honors native aggregate incompleteness', async () => {
 const source = knowledgeReferences()
 if (typeof source.native === 'object' && 'published' in source.native && 'read' in source.native.published) source.native.published.read.outgoing = null
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue({ ...referencesReply(source), outgoing_complete: false })
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 expect(await screen.findByText('Partial reference view')).toBeTruthy()
 expect(screen.getByText('Requested references unavailable')).toBeTruthy()
 expect(screen.queryByText(/This direction was not requested/)).toBeNull()
})
it('shows projection warnings once per source with all details and preserves both incomplete directions', async () => {
 const source = knowledgeReferences([])
 const warnings = Array.from({ length: 43 }, (_, index) => ({ missing_current_projection: { source: { slug: 'page-' + index, namespace: null, witness: null } } }))
 if (typeof source.native === 'object' && 'published' in source.native && 'read' in source.native.published) {
  source.native.published.read.warnings = warnings
  source.native.published.read.outgoing!.complete = false
  source.native.published.read.incoming!.complete = false
 }
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue({ ...referencesReply(source), outgoing_complete: false, incoming_complete: false })
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 const summary = await screen.findByText('Project Knowledge: 43 projection warnings')
 const disclosure = summary.closest('details')!
 expect(disclosure).not.toHaveAttribute('open')
 expect(screen.getByText('Partial reference view')).toBeTruthy()
 expect(screen.getAllByText(/Projection incomplete\./)).toHaveLength(2)
 expect(screen.queryByText('Projection warning')).toBeNull()
 await user.click(summary)
 expect(disclosure).toHaveAttribute('open')
 expect(disclosure.querySelectorAll('pre')).toHaveLength(43)
 for (let index = 0; index < 43; index++) expect(disclosure.textContent).toContain('"slug": "page-' + index + '"')
 expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(1)
})
it('requests only the focal source by default and aborts prior reads when opting project sources in and out', async () => {
 let finish!: (value: NativeReferences) => void
 vi.mocked(nativeRecordReferencesApi.read).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 await waitFor(() => expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(1))
 const initial = vi.mocked(nativeRecordReferencesApi.read).mock.calls[0]
 expect(initial[0].input.pages).toEqual([
  { source: referenceBinding.outgoing_source, direction: 'outgoing' },
  { source: referenceBinding.outgoing_source, direction: 'incoming' },
 ])
 expect(initial[0].input.include_user).toBe(false)
 expect(initial[0].input.contexts).toEqual([])
 await user.click(screen.getByRole('checkbox', { name: 'Include other project sources' }))
 await screen.findByRole('link', { name: /Target/ })
 expect(initial[1]!.aborted).toBe(true)
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[1][0].input.pages).toEqual([
  { source: { kind: 'project_task' }, direction: 'incoming' },
  { source: { kind: 'project_memory' }, direction: 'incoming' },
  { source: { kind: 'project_knowledge' }, direction: 'outgoing' },
  { source: { kind: 'project_knowledge' }, direction: 'incoming' },
 ])
 await act(async () => finish(referencesReply(knowledgeReferences([knowledgeEdge('Late prior source')]))))
 expect(screen.queryByRole('link', { name: /Late prior source/ })).toBeNull()
 await user.click(screen.getByRole('checkbox', { name: 'Include other project sources' }))
 await waitFor(() => expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(3))
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[2][0].input.pages).toEqual(initial[0].input.pages)
})
it('retries a real native request failure using the same captured binding', async () => {
 vi.mocked(nativeRecordReferencesApi.read).mockRejectedValueOnce(new Error('Native request failed (HTTP 403)'))
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 expect(await screen.findByRole('alert')).toHaveTextContent('HTTP 403')
 await user.click(screen.getByRole('button', { name: 'Retry links' }))
 await screen.findByRole('link', { name: /Target/ })
 for (const [body] of vi.mocked(nativeRecordReferencesApi.read).mock.calls) expect(body.selection).toEqual(referenceSelection)
})
it('aborts a visible read and ignores its late output', async () => {
 let finish!: (value: NativeReferences) => void
 vi.mocked(nativeRecordReferencesApi.read).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 await user.click(await screen.findByRole('button', { name: 'Cancel links' }))
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[0][1]!.aborted).toBe(true)
 await act(async () => finish(referencesReply()))
 expect(screen.queryByRole('link', { name: /Target/ })).toBeNull()
})
it('fences a late denial after changing only NativePath, preserving the new scope', async () => {
 let fail!: (cause: Error) => void
 vi.mocked(nativeRecordReferencesApi.read).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject }))
 const view = render(<RecordReferences binding={referenceBinding} />, { wrapper })
 await waitFor(() => expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(1))
 const next = { ...referenceBinding, selection: { ...referenceSelection, project_dir: "/n" } }
 view.rerender(<RecordReferences binding={next} />)
 await screen.findByRole('link', { name: /Target/ })
 await act(async () => fail(new Error('Old denied HTTP 403')))
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[0][1]!.aborted).toBe(true)
 expect(screen.queryByText('Old denied HTTP 403')).toBeNull()
})
it('continues one source/direction with the intact cursor and keeps its previous rows', async () => {
 const cursor = { transport_fixture: 9007199254740993n, internal_null: null }
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValueOnce({ ...referencesReply(knowledgeReferences([knowledgeEdge('First')], cursor)), outgoing_complete: false })
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValueOnce({ ...referencesReply(knowledgeReferences([knowledgeEdge('Second', 1)])), sources: [knowledgeReferences([knowledgeEdge('Second', 1)])] })
 const user = userEvent.setup()
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 await screen.findByRole('link', { name: /First/ })
 await user.click(screen.getByRole('button', { name: /Next page Project Knowledge outgoing/ }))
 await screen.findByRole('link', { name: /Second/ })
 expect(screen.getByRole('link', { name: /First/ })).toBeTruthy()
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[1][0].input.pages).toEqual([{ source: { kind: 'project_knowledge' }, direction: 'outgoing', cursor }])
})
it('keeps USER source unrequested until explicit opt-in and omits PROJECT selection', async () => {
 const binding = { selection: null, focus: { kind: 'memory' as const, scope: { user: 'observed-own-scope' }, id: 'm' }, outgoing_source: { kind: 'user_memory' as const } }
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue({ selection: null, sources: [{ source: { kind: 'user_memory' }, native: 'no_corpus', navigation: { outgoing: [], incoming: [] } }], outgoing_complete: true, incoming_complete: true, warnings: [] })
 const user = userEvent.setup()
 render(<RecordReferences binding={binding} />, { wrapper })
 await screen.findByText('Personal Memory references are not requested')
 expect(nativeRecordReferencesApi.read).not.toHaveBeenCalled()
 expect(screen.queryByRole('checkbox', { name: 'Include other project sources' })).toBeNull()
 await user.click(screen.getByRole('checkbox', { name: 'Include personal Memory references' }))
 await screen.findAllByText('This source has no published corpus.')
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[0][0].selection).toBeUndefined()
 expect(vi.mocked(nativeRecordReferencesApi.read).mock.calls[0][0].input.include_user).toBe(true)
})

it('includes the observed focal imported artifact without opting into other sources', async () => {
 const source = { kind: 'imported_knowledge' as const, artifact: { id: 'library/reference', version: 'release/native', context_alias: 'mounted' } }
 const focus = { kind: 'knowledge' as const, scope: { project: 'namespace-B' }, id: 'Imported page', context: 'literal namespace' }
 vi.mocked(nativeRecordReferencesApi.read).mockResolvedValue({ selection: null, sources: [{ source, native: 'no_corpus', navigation: { outgoing: [], incoming: [] } }], outgoing_complete: true, incoming_complete: true, warnings: [] })
 render(<RecordReferences binding={{ selection: null, outgoing_source: source, focus }} />, { wrapper })
 await waitFor(() => expect(nativeRecordReferencesApi.read).toHaveBeenCalled())
 expect(nativeRecordReferencesApi.read).toHaveBeenCalledWith({ input: { focus, outgoing_source: source, page_size: 20, include_user: false, contexts: [source.artifact], pages: [{ source, direction: 'outgoing' }, { source, direction: 'incoming' }] } }, expect.any(AbortSignal))
 expect(screen.queryByLabelText('Include other project sources')).toBeNull()
})

it('keeps a Knowledge generation conflict causal until an explicit retry', async () => {
 const user = userEvent.setup()
 vi.mocked(nativeRecordReferencesApi.read).mockRejectedValueOnce(new NativeRecordReadError(409, 'knowledge_generation_changed'))
 render(<RecordReferences binding={referenceBinding} />, { wrapper })
 expect(await screen.findByText('Record request failed (HTTP 409, knowledge_generation_changed)')).toBeTruthy()
 expect(screen.getByText('References unavailable')).toBeTruthy()
 expect(screen.queryByRole('link')).toBeNull()
 expect(screen.queryByText('No links in this requested source.')).toBeNull()
 expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(1)
 await user.click(screen.getByRole('button', { name: 'Retry links' }))
 expect(await screen.findByRole('link', { name: /Target/ })).toBeTruthy()
 expect(nativeRecordReferencesApi.read).toHaveBeenCalledTimes(2)
})
