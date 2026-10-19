import '@/test/contextControls'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { createMemoryRouter, MemoryRouter, Route, RouterProvider, Routes } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { nativeMemoryApi, type NativeMemory, type NativeMemoryHistory, type NativeMemorySearch } from '@/api/nativeMemory'
import { nativeMemoryMutationApi, MemoryMutationError, type MemoryMutationResult } from '@/api/nativeMemoryMutation'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { parseNativeJson } from '@/api/nativeJson'
import { useTaskWorkspace } from '@/components/task/taskWorkspace'
import { useAppStore } from '@/store/appStore'
import MemoryExplorerPage from './MemoryExplorerPage'
vi.mock('@/api/nativeMemory', async original => ({ ...await original<typeof import('@/api/nativeMemory')>(), nativeMemoryApi: { list: vi.fn(), history: vi.fn(), get: vi.fn(), search: vi.fn() } }))
vi.mock('@/api/nativeMemoryMutation', async original => ({ ...await original<typeof import('@/api/nativeMemoryMutation')>(), nativeMemoryMutationApi: { create: vi.fn(), update: vi.fn(), remove: vi.fn() } }))
const selection = { project_dir: "/ÿp", project_id: 'project1', project_version: ' literal version ' }
const generation = { sdk_version: 18446744073709551615n, project_id: 'project1', project_version: selection.project_version, physical_branch: null, inherited_target: null }
const memory: NativeMemory = { id: 'memory1', revision: 9007199254740993n, title: 'Literal title', body: '# Literal title\n\n**Guidance** with [Source](https://example.com/source)\n', attributes: { classification: 'decision', important: true, mandatory: false }, tags: ['literal,tag', 'other'], references: [], reference_generation: 1, created_by: 'fixture creator', updated_by: 'fixture updater', created_at: 0, updated_at: 1 }
const history: NativeMemoryHistory = { creation: { actor: 'fixture creator', project_id: 'project1', operation_version: selection.project_version, input: { idempotency_key: 'fixture', title: memory.title, body: memory.body, attributes: memory.attributes, tags: memory.tags, references: [] }, memory }, current: memory, deleted: false, revisions: [], generation }
const published = { state: 'published' as const, generation, memories: [memory] }
const headerReceipt: MemoryMutationResult = { delivery: 'headers', http_status: 500, receipt: { state: 'confirmed', id: memory.id, revision: memory.revision, sdk_version: generation.sdk_version, response_error: 'response_limit', completion_error: null, delivery_error: 'cancelled' } }
function page(path = projectRecordHref('memory', memory.id, selection)) { return render(<MemoryRouter initialEntries={[path]}><Routes><Route path="/memory/explorer/:scopeId/:memoryId?" element={<MemoryExplorerPage />} /></Routes></MemoryRouter>) }
function routedPage() {
 const router = createMemoryRouter([{ path: '/memory/explorer/:scopeId/:memoryId?', element: <MemoryExplorerPage /> }], { initialEntries: [projectRecordHref('memory', memory.id, selection)] })
 render(<RouterProvider router={router} />)
 return router
}
beforeEach(() => {
 vi.resetAllMocks()
 vi.stubGlobal('matchMedia', vi.fn(() => ({ matches: false, addEventListener: vi.fn(), removeEventListener: vi.fn() })))
 useTaskWorkspace.setState({ module: 'memory', selection, contexts: [selection], phase: 'ready', error: '' })
 useAppStore.setState({ activeProjectDir: '/unrelated', loadProjects: vi.fn(async () => {}) })
 vi.mocked(nativeMemoryApi.list).mockResolvedValue({ selection, native: published })
 vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: history })
 vi.mocked(nativeMemoryApi.search).mockResolvedValue({ selection, native: { mode: 'fts', model: null, state: 'published', generation, total_hits: 1, hits: [{ current: memory, matched: memory, location: { kind: 'current' }, deleted: false, score: 0.25 }] } })
 vi.mocked(nativeMemoryMutationApi.create).mockResolvedValue(headerReceipt)
 vi.mocked(nativeMemoryMutationApi.update).mockResolvedValue(headerReceipt)
 vi.mocked(nativeMemoryMutationApi.remove).mockResolvedValue(headerReceipt)
})
describe('native Memory product journeys', () => {
 it('keeps PROJECT idle without its own selection while USER remains independent', async () => {
  useTaskWorkspace.setState({ selection: null, phase: 'forbidden' })
  const idle = page('/memory/explorer/project'); await screen.findByText('Select a working context')
  expect(nativeMemoryApi.list).not.toHaveBeenCalled(); idle.unmount()
  vi.mocked(nativeMemoryApi.list).mockResolvedValue({ scope_id: 'fixture-user', currentOwner: { installation_id: 'fixture', identity: { issuer: 'fixture', subject: 'fixture' } }, native: { state: 'no_corpus' } })
  page('/memory/explorer/user'); await screen.findByText('No Memory corpus has been published')
  expect(nativeMemoryApi.list).toHaveBeenCalledWith({ kind: 'user', requested_scope_id: undefined }, {}, expect.any(AbortSignal))
  expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
 })
 it('clears prior PROJECT guidance when the own-module context is removed', async () => {
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  await act(async () => useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'forbidden' }))
  expect(screen.queryByRole('button', { name: memory.title })).toBeNull()
  expect(screen.getByText('Select a working context')).toBeTruthy()
 })
 it('uses exact physical selection and SDK ordering without priority or date sorting', async () => {
  const other = { ...memory, id: 'memory2', title: 'Second SDK row', updated_at: 999, attributes: { ...memory.attributes, mandatory: true } }
  vi.mocked(nativeMemoryApi.list).mockResolvedValue({ selection, native: { ...published, memories: [memory, other] } })
  page('/memory/explorer/project'); await screen.findByRole('button', { name: other.title })
  expect(within(screen.getByRole('table')).getAllByRole('row').slice(1).map(row => within(row).getByRole('button').textContent)).toEqual([memory.title, other.title])
  expect(nativeMemoryApi.list).toHaveBeenCalledWith({ kind: 'project', selection }, {}, expect.any(AbortSignal))
 })
 it('keeps complete body and history mounted when the current row is clicked again', async () => {
  page(); await screen.findByRole('article', { name: 'Memory record' })
  const count = vi.mocked(nativeMemoryApi.history).mock.calls.length
  expect(within(screen.getByRole('article')).getAllByRole('heading', { name: memory.title })).toHaveLength(2)
  await userEvent.setup().click(within(screen.getByRole('table')).getByRole('button', { name: memory.title }))
  expect(nativeMemoryApi.history).toHaveBeenCalledTimes(count)
  expect(parseNativeJson(screen.getByText('Complete history document').parentElement!.querySelector('pre')!.textContent!)).toEqual(history)
 })
 it('renders rich snippet links outside the record button', async () => {
  page('/memory/explorer/project'); const link = await screen.findByRole('link', { name: 'Source' })
  expect(link.closest('button')).toBeNull(); expect(screen.getAllByText('Guidance').some(value => value.tagName === 'STRONG')).toBe(true)
 })
 it('forwards native boolean and classification filters inside the real search input', async () => {
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  const user = userEvent.setup()
  await user.click(screen.getByLabelText('Filter memory type')); await user.click(screen.getByRole('option', { name: 'decision' }))
  await user.click(screen.getByLabelText('Filter importance')); await user.click(screen.getByRole('option', { name: 'Not important' }))
  await user.type(screen.getByLabelText('Search memories'), ' literal query ')
  await waitFor(() => expect(nativeMemoryApi.search).toHaveBeenLastCalledWith({ kind: 'project', selection }, { text: ' literal query ', top_k: 20, mode: 'fts', filter: { classification: 'decision', important: false }, include_deleted_history: false }, expect.any(AbortSignal)))
  await screen.findByText('Complete search result')
 })
 it('creates with closed attributes and displays confirmed HTTP500 effect without retrying', async () => {
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Create memory' }))
  const form = screen.getByRole('dialog', { name: 'Create memory' })
  await user.type(within(form).getByLabelText('Memory title'), ' New title ')
  await user.type(within(form).getByLabelText('Memory content'), '# New body\n')
  await user.click(within(form).getByRole('button', { name: 'Create memory' }))
  await screen.findByText('Memory write effect confirmed')
  expect(nativeMemoryMutationApi.create).toHaveBeenCalledOnce()
  expect(nativeMemoryMutationApi.create).toHaveBeenCalledWith({ kind: 'project', selection }, expect.objectContaining({ title: ' New title ', body: '# New body\n', attributes: { classification: 'fact', important: false, mandatory: false }, references: [], idempotency_key: expect.any(String) }), expect.any(AbortSignal))
  expect(screen.getByText('Response error').nextElementSibling?.textContent).toBe('response_limit')
 })
 it('edits with exact CAS, a reason and untouched literal tags/body', async () => {
  page(); await screen.findByRole('button', { name: 'Edit' })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Edit' }))
  const form = screen.getByRole('dialog', { name: 'Edit memory' })
  await user.clear(within(form).getByLabelText('Memory title')); await user.type(within(form).getByLabelText('Memory title'), 'Revised title')
  await user.click(within(form).getByRole('button', { name: 'Save revision' }))
  await screen.findByText('Memory write effect confirmed')
  expect(nativeMemoryMutationApi.update).toHaveBeenCalledWith({ kind: 'project', selection }, expect.objectContaining({ id: memory.id, expected_revision: 9007199254740993n, title: 'Revised title', body: memory.body, tags: memory.tags, reason: 'Updated from Memory explorer', idempotency_key: expect.any(String) }), expect.any(AbortSignal))
 })
 it('refreshes confirmed history for the same ID before another CAS edit', async () => {
  page(); await screen.findByRole('button', { name: 'Edit' })
  const next = { ...memory, revision: 9007199254740994n, title: 'Committed title' }
  vi.mocked(nativeMemoryMutationApi.update).mockImplementationOnce(async () => {
   vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: { ...history, current: next, revisions: [{ actor: 'fixture updater', project_id: 'project1', operation_version: selection.project_version, operation: { update: { expected_revision: memory.revision, reason: 'Fixture revision' } }, before: memory, after: next }] } })
   return { ...headerReceipt, receipt: { ...(headerReceipt.delivery === 'headers' ? headerReceipt.receipt : {}), state: 'confirmed', id: memory.id, revision: next.revision, sdk_version: generation.sdk_version, response_error: 'response_limit', completion_error: null, delivery_error: null }, delivery: 'headers', http_status: 500 }
  })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Edit' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Save revision' }))
  await screen.findByRole('heading', { name: next.title })
  await user.click(screen.getByRole('button', { name: 'Edit' }))
  await user.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Save revision' }))
  await waitFor(() => expect(nativeMemoryMutationApi.update).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeMemoryMutationApi.update).mock.calls[1][1].expected_revision).toBe(next.revision)
 })
 it('preserves comma-containing tags when adding another literal tag', async () => {
  page(); await screen.findByRole('button', { name: 'Edit' })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Edit' }))
  const form = screen.getByRole('dialog'); await user.click(within(form).getByRole('button', { name: 'Add tag' }))
  await user.type(within(form).getByLabelText('Tag 3'), 'new,tag')
  await user.click(within(form).getByRole('button', { name: 'Save revision' })); await screen.findByText('Memory write effect confirmed')
  expect(vi.mocked(nativeMemoryMutationApi.update).mock.calls[0][1].tags).toEqual(['literal,tag', 'other', 'new,tag'])
 })
 it('uses a fresh idempotency key for a new deliberately identical create intent after confirmation', async () => {
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  const user = userEvent.setup()
  for (let attempt = 0; attempt < 2; attempt++) {
   await user.click(screen.getByRole('button', { name: 'Create memory' }))
   const form = screen.getByRole('dialog'); await user.type(within(form).getByLabelText('Memory title'), 'Identical title'); await user.type(within(form).getByLabelText('Memory content'), 'Identical body')
   await user.click(within(form).getByRole('button', { name: 'Create memory' })); await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  }
  expect(nativeMemoryMutationApi.create).toHaveBeenCalledTimes(2)
  expect(vi.mocked(nativeMemoryMutationApi.create).mock.calls[0][1].idempotency_key).not.toBe(vi.mocked(nativeMemoryMutationApi.create).mock.calls[1][1].idempotency_key)
 })
 it('requires a removal reason and sends CAS only after explicit confirmation', async () => {
  page(); await screen.findByRole('button', { name: 'Remove' })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   let dialog = screen.getByRole('dialog', { name: 'Remove memory' })
   expect((within(dialog).getByRole('button', { name: 'Remove memory' }) as HTMLButtonElement).disabled).toBe(true)
   await user.type(within(dialog).getByLabelText('Memory removal reason'), '   ')
   expect((within(dialog).getByRole('button', { name: 'Remove memory' }) as HTMLButtonElement).disabled).toBe(true)
   await user.click(within(dialog).getByRole('button', { name: 'Cancel' }))
  expect(nativeMemoryMutationApi.remove).not.toHaveBeenCalled()
   await user.click(screen.getByRole('button', { name: 'Remove' })); await user.keyboard('{Escape}')
   expect(screen.queryByRole('dialog')).toBeNull(); expect(nativeMemoryMutationApi.remove).not.toHaveBeenCalled()
   await user.click(screen.getByRole('button', { name: 'Remove' })); dialog = screen.getByRole('dialog')
   await user.type(within(dialog).getByLabelText('Memory removal reason'), ' Retired guidance ')
   await user.click(within(dialog).getByRole('button', { name: 'Remove memory' })); await screen.findByText('Memory write effect confirmed')
   expect(nativeMemoryMutationApi.remove).toHaveBeenCalledOnce()
   expect(nativeMemoryMutationApi.remove).toHaveBeenCalledWith({ kind: 'project', selection }, expect.objectContaining({ id: memory.id, expected_revision: memory.revision, reason: ' Retired guidance ', idempotency_key: expect.any(String) }), expect.any(AbortSignal))
   expect(screen.queryByRole('dialog')).toBeNull()
  })
  it('retains the removal reason and operation key on explicit retry and prevents duplicate pending confirmation', async () => {
   let finish!: (result: MemoryMutationResult) => void
   vi.mocked(nativeMemoryMutationApi.remove).mockRejectedValueOnce(new Error('Removal denied')).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
   page(); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   const dialog = screen.getByRole('dialog'), reason = within(dialog).getByLabelText('Memory removal reason')
   await user.type(reason, 'Retire this guidance'); await user.click(within(dialog).getByRole('button', { name: 'Remove memory' }))
   await within(dialog).findByText(/Removal denied/)
   expect((reason as HTMLInputElement).value).toBe('Retire this guidance')
   await user.click(within(dialog).getByRole('button', { name: 'Remove memory' }))
   expect((within(dialog).getByRole('button', { name: 'Removing…' }) as HTMLButtonElement).disabled).toBe(true)
   await user.keyboard('{Enter}')
   expect(nativeMemoryMutationApi.remove).toHaveBeenCalledTimes(2)
   expect(vi.mocked(nativeMemoryMutationApi.remove).mock.calls[0][1].idempotency_key).toBe(vi.mocked(nativeMemoryMutationApi.remove).mock.calls[1][1].idempotency_key)
   await act(async () => finish(headerReceipt)); await screen.findByText('Memory write effect confirmed')
  })
  it('invalidates an open confirmation when the module selection is withdrawn', async () => {
   page('/memory/explorer/project/memory1'); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await user.type(screen.getByLabelText('Memory removal reason'), 'Obsolete scope')
   await act(async () => useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'forbidden' }))
   expect(screen.queryByRole('dialog')).toBeNull(); expect(nativeMemoryMutationApi.remove).not.toHaveBeenCalled()
   await act(async () => useTaskWorkspace.setState({ selection, contexts: [selection], phase: 'ready' }))
   await screen.findByRole('button', { name: 'Remove' }); await user.click(screen.getByRole('button', { name: 'Remove' }))
   expect((screen.getByLabelText('Memory removal reason') as HTMLInputElement).value).toBe('')
  })
  it('invalidates confirmation when refreshed history has a newer revision', async () => {
   const router = routedPage(); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   const current = { ...memory, revision: 9007199254740994n }
   vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: { ...history, current,
    revisions: [{ actor: 'fixture updater', project_id: 'project1', operation_version: selection.project_version,
     operation: { update: { expected_revision: memory.revision, reason: 'External revision' } }, before: memory, after: current }] } })
   await act(async () => router.navigate(projectRecordHref('memory', memory.id, selection) + '&refresh=1'))
   await screen.findByRole('button', { name: 'Revision 9007199254740994' })
   expect(screen.queryByRole('dialog')).toBeNull(); expect(nativeMemoryMutationApi.remove).not.toHaveBeenCalled()
  })
  it('keeps late removal errors from contaminating a different record confirmation', async () => {
   let fail!: (error: Error) => void
   vi.mocked(nativeMemoryMutationApi.remove).mockImplementationOnce(() => new Promise((_resolve, reject) => { fail = reject }))
   const router = routedPage(); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await user.type(screen.getByLabelText('Memory removal reason'), 'First record'); await user.click(screen.getByRole('button', { name: 'Remove memory' }))
   const other = { ...memory, id: 'memory2', title: 'Other record' }
   vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: { ...history, current: other, creation: { ...history.creation, memory: other } } })
   await act(async () => router.navigate(projectRecordHref('memory', other.id, selection)))
   await screen.findByRole('button', { name: 'Remove' }); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await act(async () => fail(new Error('Late first-record failure')))
   const dialog = screen.getByRole('dialog', { name: 'Remove memory' })
   expect(within(dialog).getByText('Other record')).toBeTruthy()
   expect(screen.queryByText(/Late first-record failure/)).toBeNull()
   expect(router.state.location.pathname).toBe('/memory/explorer/project/memory2')
  })
  it('settles a confirmed former removal without closing or contaminating the new record intent', async () => {
   let finish!: (result: MemoryMutationResult) => void
   const other = { ...memory, id: 'memory2', title: 'Other record' }
   vi.mocked(nativeMemoryMutationApi.remove).mockImplementationOnce(() => new Promise(resolve => { finish = resolve }))
    .mockResolvedValueOnce({ ...headerReceipt, receipt: { ...headerReceipt.receipt, id: other.id } })
   const router = routedPage(); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await user.type(screen.getByLabelText('Memory removal reason'), 'First intent'); await user.click(screen.getByRole('button', { name: 'Remove memory' }))
   vi.mocked(nativeMemoryApi.history).mockResolvedValue({ selection, native: { ...history, current: other, creation: { ...history.creation, memory: other } } })
   await act(async () => router.navigate(projectRecordHref('memory', other.id, selection)))
   await screen.findByRole('button', { name: 'Remove' }); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await act(async () => finish(headerReceipt))
   const dialog = screen.getByRole('dialog', { name: 'Remove memory' })
   expect(within(dialog).getByText('Other record')).toBeTruthy()
   expect(screen.queryByText('Memory write effect confirmed')).toBeNull()
   await user.type(within(dialog).getByLabelText('Memory removal reason'), 'Second intent')
   await user.click(within(dialog).getByRole('button', { name: 'Remove memory' })); await screen.findByText('Memory write effect confirmed')
   const calls = vi.mocked(nativeMemoryMutationApi.remove).mock.calls
   expect(calls[1][1]).toEqual(expect.objectContaining({ id: other.id, expected_revision: other.revision, reason: 'Second intent' }))
   expect(calls[1][1].idempotency_key).not.toBe(calls[0][1].idempotency_key)
  })
  it('retains an uncertain removal receipt without navigating or automatically retrying', async () => {
   vi.mocked(nativeMemoryMutationApi.remove).mockResolvedValue({ ...headerReceipt, receipt: { ...headerReceipt.receipt, state: 'uncertain' } })
   page(); await screen.findByRole('button', { name: 'Remove' })
   const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Remove' }))
   await user.type(screen.getByLabelText('Memory removal reason'), 'Retired guidance'); await user.click(screen.getByRole('button', { name: 'Remove memory' }))
   await within(screen.getByRole('dialog')).findByText('Memory write outcome uncertain')
   expect(nativeMemoryMutationApi.remove).toHaveBeenCalledOnce()
   expect(screen.getByRole('article', { name: 'Memory record' })).toBeTruthy()
 })
 it('keeps the draft and stable operation key after CAS failure without automatic retry', async () => {
  vi.mocked(nativeMemoryMutationApi.update).mockRejectedValue(new MemoryMutationError(409, 'revision_conflict', memory.revision, 9007199254740994n))
  page(); await screen.findByRole('button', { name: 'Edit' })
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Edit' }))
  const save = within(screen.getByRole('dialog')).getByRole('button', { name: 'Save revision' })
  await user.click(save); await screen.findByText('Unable to complete Memory write')
  expect(nativeMemoryMutationApi.update).toHaveBeenCalledOnce()
  await user.click(save); await waitFor(() => expect(nativeMemoryMutationApi.update).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeMemoryMutationApi.update).mock.calls[0][1].idempotency_key).toBe(vi.mocked(nativeMemoryMutationApi.update).mock.calls[1][1].idempotency_key)
 })
 it('distinguishes unavailable semantic search from empty keyword publication', async () => {
  vi.mocked(nativeMemoryApi.search).mockResolvedValueOnce({ selection, native: { mode: 'semantic', model: null, state: 'unavailable', reason: 'model_unavailable' } })
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  const user = userEvent.setup(); await user.click(screen.getByLabelText('Memory search method')); await user.click(screen.getByRole('option', { name: 'Semantic' })); await user.type(screen.getByLabelText('Search memories'), 'guidance')
  await screen.findByText('Memory search unavailable'); expect(screen.queryByText('No memories match this view')).toBeNull()
  vi.mocked(nativeMemoryApi.search).mockResolvedValue({ selection, native: { mode: 'fts', model: null, state: 'published_empty', generation } })
  await user.click(screen.getByLabelText('Memory search method')); await user.click(screen.getByRole('option', { name: 'Keyword' })); await screen.findByText('The Memory search publication is empty')
 })
 it('aborts superseded search and ignores a late former-query result', async () => {
  let finish!: (value: { selection: typeof selection; native: NativeMemorySearch }) => void
  vi.mocked(nativeMemoryApi.search).mockImplementationOnce(() => new Promise(done => { finish = done }))
  page('/memory/explorer/project'); await screen.findByRole('button', { name: memory.title })
  const user = userEvent.setup(), query = screen.getByLabelText('Search memories')
  await user.type(query, 'first'); await waitFor(() => expect(nativeMemoryApi.search).toHaveBeenCalledOnce())
  const old = vi.mocked(nativeMemoryApi.search).mock.calls[0][2]!
  await user.clear(query); await user.type(query, 'latest'); await waitFor(() => expect(nativeMemoryApi.search).toHaveBeenCalledTimes(2))
  expect(old.aborted).toBe(true)
  await act(async () => finish({ selection, native: { mode: 'fts', model: null, state: 'published', generation, total_hits: 1, hits: [{ current: { ...memory, title: 'Stale search row' }, matched: memory, location: { kind: 'current' }, deleted: false, score: 0.1 }] } }))
  expect(screen.queryByRole('button', { name: 'Stale search row' })).toBeNull()
 })
 it('keeps USER write-only receipts visible without rereading or discovering PROJECT', async () => {
  vi.mocked(nativeMemoryApi.list).mockRejectedValue(new Error('HTTP 403'))
  page('/memory/explorer/user'); await screen.findByText('HTTP 403')
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Create memory' }))
  const form = screen.getByRole('dialog'); await user.type(within(form).getByLabelText('Memory title'), 'Own guidance'); await user.type(within(form).getByLabelText('Memory content'), 'Own body')
  await user.click(within(form).getByRole('button', { name: 'Create memory' })); await screen.findByText('Memory write effect confirmed')
  expect(nativeMemoryMutationApi.create).toHaveBeenCalledWith({ kind: 'user', requested_scope_id: undefined }, expect.any(Object), expect.any(AbortSignal))
  expect(nativeMemoryApi.list).toHaveBeenCalledOnce(); expect(nativeMemoryApi.history).not.toHaveBeenCalled(); expect(useAppStore.getState().loadProjects).not.toHaveBeenCalled()
 })
 it('keeps a not-applied create draft and intent without navigation or automatic retry', async () => {
  const notApplied: MemoryMutationResult = { delivery: 'headers', http_status: 500, receipt: { state: 'not_applied', id: null, revision: null, sdk_version: null, index_sdk_version: 18446744073709551615n, completion_error: 'revision_conflict', response_error: 'response_limit', delivery_error: null } }
  vi.mocked(nativeMemoryMutationApi.create).mockResolvedValue(notApplied)
  const router = routedPage(); await screen.findByRole('button', { name: 'Edit' })
  const initialPath = router.state.location.pathname
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Create memory' }))
  const form = screen.getByRole('dialog', { name: 'Create memory' })
  await user.type(within(form).getByLabelText('Memory title'), 'Pending title')
  await user.type(within(form).getByLabelText('Memory content'), 'Pending guidance')
  await user.click(within(form).getByRole('button', { name: 'Create memory' }))
  await screen.findByText('Memory write not applied')
  expect(nativeMemoryMutationApi.create).toHaveBeenCalledOnce()
  expect(router.state.location.pathname).toBe(initialPath)
  expect((within(form).getByLabelText('Memory title') as HTMLInputElement).value).toBe('Pending title')
  expect(screen.queryByText('Memory write completed')).toBeNull()
  expect(screen.getByText('Index SDK version').nextElementSibling?.textContent).toBe('18446744073709551615')
  const key = vi.mocked(nativeMemoryMutationApi.create).mock.calls[0][1].idempotency_key
  await user.click(within(form).getByRole('button', { name: 'Create memory' }))
  await waitFor(() => expect(nativeMemoryMutationApi.create).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeMemoryMutationApi.create).mock.calls[1][1].idempotency_key).toBe(key)
 })
 it('preserves removal confirmation and target after an index-only receipt', async () => {
  vi.mocked(nativeMemoryMutationApi.remove).mockResolvedValue({ delivery: 'headers', http_status: 500, receipt: { state: 'not_applied', id: memory.id, revision: null, sdk_version: null, index_sdk_version: 9007199254740993n, completion_error: 'request_cancelled_or_expired', response_error: 'response_limit', delivery_error: null } })
  const router = routedPage(); await screen.findByRole('button', { name: 'Remove' })
  const initialPath = router.state.location.pathname, user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: /^Remove$/ }))
  const dialog = screen.getByRole('dialog', { name: 'Remove memory' })
  await user.type(within(dialog).getByLabelText('Memory removal reason'), 'Retire explicitly')
  await user.click(within(dialog).getByRole('button', { name: 'Remove memory' }))
  await within(dialog).findByText('Memory write not applied')
  expect(nativeMemoryMutationApi.remove).toHaveBeenCalledOnce()
  expect(router.state.location.pathname).toBe(initialPath)
  expect((within(dialog).getByLabelText('Memory removal reason') as HTMLInputElement).value).toBe('Retire explicitly')
  expect(screen.queryByText('Memory deleted')).toBeNull()
  const key = vi.mocked(nativeMemoryMutationApi.remove).mock.calls[0][1].idempotency_key
  await user.click(within(dialog).getByRole('button', { name: 'Remove memory' }))
  await waitFor(() => expect(nativeMemoryMutationApi.remove).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeMemoryMutationApi.remove).mock.calls[1][1].idempotency_key).toBe(key)
 })

 it('keeps an index-only edit and gives a changed payload a fresh explicit intent', async () => {
  const index = { sdk_version: 18446744073709551615n, generation: null, recovery_key: { corpus_uri: 'index-corpus', physical_branch: null, base_version: 9007199254740993n, transaction_id: 'index-transaction' } }
  vi.mocked(nativeMemoryMutationApi.update).mockImplementation(async (_scope, input) => ({ delivery: 'receipt', envelope: { selection, native: { state: 'not_applied', id: memory.id, revision: null, idempotency_key: input.idempotency_key, sdk_version: null, generation: null, recovery_key: null, index_publication: index, completion_error: 'revision_conflict', delivery_error: null } } }))
  const router = routedPage(); await screen.findByRole('button', { name: 'Edit' })
  const path = router.state.location.pathname, reads = vi.mocked(nativeMemoryApi.history).mock.calls.length
  const user = userEvent.setup(); await user.click(screen.getByRole('button', { name: 'Edit' }))
  const form = screen.getByRole('dialog', { name: 'Edit memory' })
  await user.click(within(form).getByRole('button', { name: 'Save revision' }))
  await within(form).findByText('Memory write not applied')
  expect(nativeMemoryMutationApi.update).toHaveBeenCalledOnce()
  expect(nativeMemoryApi.history).toHaveBeenCalledTimes(reads)
  expect(router.state.location.pathname).toBe(path)
  expect((within(form).getByLabelText('Memory title') as HTMLInputElement).value).toBe(memory.title)
  const key = vi.mocked(nativeMemoryMutationApi.update).mock.calls[0][1].idempotency_key
  await user.type(within(form).getByLabelText('Memory title'), ' changed')
  await user.click(within(form).getByRole('button', { name: 'Save revision' }))
  await waitFor(() => expect(nativeMemoryMutationApi.update).toHaveBeenCalledTimes(2))
  expect(vi.mocked(nativeMemoryMutationApi.update).mock.calls[1][1].idempotency_key).not.toBe(key)
 })

 it.each(['Create memory', 'Edit'])('clears the former receipt when opening a new %s intent', async action => {
  page(); await screen.findByRole('button', { name: 'Edit' })
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Edit' }))
  await user.click(within(screen.getByRole('dialog', { name: 'Edit memory' })).getByRole('button', { name: 'Save revision' }))
  await screen.findByText('Memory write effect confirmed')
  expect(nativeMemoryMutationApi.update).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: action }))
  const form = screen.getByRole('dialog', { name: action === 'Edit' ? 'Edit memory' : 'Create memory' })
  expect(within(form).queryByText('Memory write effect confirmed')).toBeNull()
  expect(within(form).queryByText('Write receipt')).toBeNull()
  expect(nativeMemoryMutationApi.update).toHaveBeenCalledOnce()
  expect(nativeMemoryMutationApi.create).not.toHaveBeenCalled()
 })

})
