import '@/test/contextControls'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi, TaskReadError, type TaskCataloguePage, type TaskDetail, type TaskSearchPage } from '@/api/task'
import { parseNativeJson, stringifyNativeJson } from '@/api/nativeJson'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { useTaskWorkspace } from './taskWorkspaceState'
import TaskExplorerPage from './TaskExplorerPage'
import { WorkspaceRefreshProvider } from '@/components/layout/WorkspaceRefreshProvider'
import { TaskWorkspaceSelectors } from './TaskWorkspace'
import { context, cursor, detail, first, second, secondDetail, historyPage, commentsPage, summary } from './task.test-fixtures'
vi.mock('@/api/task', async importOriginal => ({ ...await importOriginal<typeof import('@/api/task')>(), taskApi: { contexts: vi.fn(), context: vi.fn(), catalogue: vi.fn(), search: vi.fn(), get: vi.fn() } }))
vi.mock('@/components/shared/RecordReferences', () => ({ RecordReferences: () => <h3>Record links</h3> }))
function Location() { return <output data-testid="location">{useLocation().pathname}</output> }
function page() { return render(<MemoryRouter initialEntries={['/task/explorer']}><Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /><Route path="/task/sessions/:id?" element={<Location />} /><Route path="/task/gantt" element={<Location />} /></Routes></MemoryRouter>) }
beforeEach(() => {
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
 vi.clearAllMocks()
 useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready', error: '' })
 vi.mocked(taskApi.context).mockImplementation(async selection => ({ ...context, ...selection }))
 vi.mocked(taskApi.catalogue).mockImplementation(async options => options.cursor ? { results: [summary(second)], next_cursor: null } : { results: [summary(first)], next_cursor: cursor })
 vi.mocked(taskApi.search).mockImplementation(async (options): Promise<TaskSearchPage> => ({ results: options.cursor ? [summary(second)] : [summary(first)], next_cursor: options.cursor ? null : { binding: { mode: options.mode }, offset: 1 }, total_hits: 2, matches: options.cursor ? { task2: { location: { Specification: { revision: 1 } }, score: 0.8 } } : { task1: { location: { Comment: { comment_id: 'comment1', sequence: 1 } }, score: 0.9 } } }))
 vi.mocked(taskApi.get).mockImplementation(async (_, id, options) => {
  const current = id === 'task1' ? detail : secondDetail
  return { ...current, history: options?.history ? id === 'task1' ? historyPage : { before: second, events: [], next_cursor: null } : null, comments: options?.comments ? id === 'task1' ? commentsPage : { comments: [], next_cursor: null } : null }
 })
})
describe('native Task Explorer', () => {
 it('shows the associated commit and its lifecycle change without changing completion evidence', async () => {
  const sha = 'a'.repeat(40)
  const task = { ...first, commit: sha, status: 'completed' as const }
  const updated = { ...detail, task }
  vi.mocked(taskApi.get).mockImplementation(async (_, __, options) => ({ ...updated,
   history: options?.history ? { before: first, events: [{ actor: 'agent-a', kind: { commit_updated: { reason: 'Record successful amend' } }, observed_at: 15, record: task }], next_cursor: null } : null,
   comments: options?.comments ? commentsPage : null,
  }))
  const user = userEvent.setup(); page()
  await screen.findByRole('heading', { name: 'Specification' })
  expect(screen.getByText(sha)).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  expect(await screen.findByText('Record successful amend')).toBeTruthy()
  expect(screen.getAllByText(sha)).toHaveLength(2)
 })
 it('renders exact u64 revisions and exports the complete native document', async () => {
  const large = { ...detail, task: { ...first, ownership: { ...first.ownership, claim_epoch: 18446744073709551615n }, specification: { ...first.specification, revision: 9007199254740993n } } }
  vi.mocked(taskApi.get).mockImplementation(async (_, __, options) => ({ ...large, history: options?.history ? historyPage : null, comments: options?.comments ? commentsPage : null }))
  const user = userEvent.setup()
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:exact') })
  Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() })
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  page(); await screen.findByRole('heading', { name: 'Specification' })
  expect(screen.getByText('Claim epoch').nextElementSibling?.textContent).toBe('18446744073709551615')
  const json = screen.getByText('Current JSON document').parentElement?.querySelector('pre')
  expect(json?.textContent).toContain('"revision": 9007199254740993')
  expect(json?.textContent).toContain('"claim_epoch": 18446744073709551615')
  await user.click(screen.getByRole('button', { name: 'Export complete JSON' }))
  const blob = vi.mocked(URL.createObjectURL).mock.calls[0][0] as Blob
  const text = await new Promise<string>(done => { const reader = new FileReader(); reader.onload = () => done(String(reader.result)); reader.readAsText(blob) })
  expect(text).toContain('"claim_epoch":18446744073709551615')
  expect(text).not.toContain('18446744073709552000')
 })
 it('waits for project validation and rejects late results after deselection', async () => {
  let resolve: (value: TaskCataloguePage) => void = () => {}
  vi.mocked(taskApi.catalogue).mockImplementation(() => new Promise(done => { resolve = done }))
  useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'loading', error: '' }); page()
  expect(screen.getByText('Loading working contexts…')).toBeTruthy()
  expect(taskApi.context).not.toHaveBeenCalled()
  act(() => useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready', error: '' }))
  await waitFor(() => expect(taskApi.catalogue).toHaveBeenCalled())
  act(() => useTaskWorkspace.setState({ selection: null, phase: 'ready' }))
  await act(async () => resolve({ results: [summary(first)], next_cursor: null }))
  expect(screen.queryByText('First task')).toBeNull()
  expect(screen.getByText('Select a working context')).toBeTruthy()
 })
 it('echoes native context and cursor, renders checks and rich current/history dates', async () => {
  const user = userEvent.setup(); page()
  await screen.findByRole('heading', { name: 'Specification' })
  expect(taskApi.context).toHaveBeenCalledWith(context, expect.any(AbortSignal))
  expect(taskApi.get).toHaveBeenCalledWith(context, 'task1', { signal: expect.any(AbortSignal) })
  expect(screen.getAllByText('observable outcome').length).toBeGreaterThan(0)
  expect(screen.queryByText('native producer')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Load comments' }))
  expect(await screen.findByText('native producer')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  await screen.findByText('rev 2 · revised')
  expect(screen.getAllByText(new Date(Number(first.created_at)).toLocaleString()).length).toBeGreaterThan(0)
  expect(screen.getAllByText(new Date(Number(first.ownership.lease_expires_at!)).toLocaleString()).length).toBeGreaterThan(0)
  expect(screen.getAllByText('npm test')[0].tagName).toBe('CODE')
  expect(screen.getAllByText(/3 calendar days/).length).toBeGreaterThan(0)
  await user.click(screen.getByText('rev 2 · revised'))
  expect(screen.getByText('Before').parentElement?.textContent).toContain('Initial specification.')
  expect(screen.getByText('No scheduled dates')).toBeTruthy()
  const catalogue = screen.getByLabelText('Task catalogue')
  await user.click(within(catalogue).getByRole('button', { name: 'Load more' }))
  expect(await within(catalogue).findByText('Second task')).toBeTruthy()
  expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ selection: context, cursor }))
  await user.click(within(catalogue).getByText('Second task'))
  expect(await screen.findByText('A follow-up.')).toBeTruthy()
 })
 it('preserves selected detail and supports native filters/search and keyboard Gantt route', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Specification' })
  const getCount = vi.mocked(taskApi.get).mock.calls.length
  await user.click(within(screen.getByLabelText('Task catalogue')).getByText('First task'))
  expect(taskApi.get).toHaveBeenCalledTimes(getCount)
  await user.click(screen.getByRole('combobox', { name: 'Filter task status' }))
  await user.click(screen.getByRole('option', { name: 'Completed' }))
  await user.type(screen.getByLabelText('Search tasks'), 'scheduler')
  await waitFor(() => expect(taskApi.search).toHaveBeenLastCalledWith(expect.objectContaining({ text: 'scheduler', mode: 'fts', filter: { status: 'completed' } })))
  const gantt = screen.getByRole('tab', { name: 'Gantt' }); gantt.focus(); await user.keyboard('{Enter}')
  expect(screen.getByTestId('location').textContent).toBe('/task/gantt')
 })
 it('uses semantic and hybrid search with native paging, filters and record navigation', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Specification' })
  await user.type(screen.getByLabelText('Search tasks'), 'publication')
  await waitFor(() => expect(taskApi.search).toHaveBeenLastCalledWith(expect.objectContaining({ text: 'publication', mode: 'fts' })))
  await user.click(screen.getByRole('combobox', { name: 'Task search mode' }))
  await user.click(screen.getByRole('option', { name: 'Semantic' }))
  await waitFor(() => expect(taskApi.search).toHaveBeenLastCalledWith(expect.objectContaining({ text: 'publication', mode: 'semantic', selection: context })))
  expect(screen.getByText(/Semantic results become available/)).toBeTruthy()
  const match = within(screen.getByLabelText('Matched location for ' + first.specification.title)).getByRole('link', { name: 'Comment #1' })
  expect(match.getAttribute('href')).toContain('#task-comments'); expect(match.getAttribute('title')).toBe('Search score 0.900')
  await user.click(within(screen.getByLabelText('Task catalogue')).getByRole('button', { name: 'Load more' }))
  expect(await within(screen.getByLabelText('Task catalogue')).findByText('Second task')).toBeTruthy()
  expect(taskApi.search).toHaveBeenLastCalledWith(expect.objectContaining({ mode: 'semantic', cursor: { binding: { mode: 'semantic' }, offset: 1 } }))
  await user.click(within(screen.getByLabelText('Task catalogue')).getByText('Second task'))
  expect(await screen.findByText('A follow-up.')).toBeTruthy()
  await user.click(screen.getByRole('combobox', { name: 'Task search mode' }))
  await user.click(screen.getByRole('option', { name: 'Hybrid' }))
  await waitFor(() => expect(taskApi.search).toHaveBeenLastCalledWith(expect.objectContaining({ text: 'publication', mode: 'hybrid', selection: context })))
 })
 it('shows a semantic service failure without substituting a title catalogue', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Specification' })
  const catalogueCalls = vi.mocked(taskApi.catalogue).mock.calls.length
  vi.mocked(taskApi.search).mockRejectedValue(new Error('Embedding service unavailable'))
  await user.type(screen.getByLabelText('Search tasks'), 'publication')
  expect(await screen.findByText('Embedding service unavailable')).toBeTruthy()
  expect(taskApi.catalogue).toHaveBeenCalledTimes(catalogueCalls)
  expect(within(screen.getByLabelText('Task catalogue')).queryByText('First task')).toBeNull()
 })
 it('shows real errors and retries detail without inventing an empty task', async () => {
  vi.mocked(taskApi.get).mockRejectedValueOnce(new Error('HTTP 403'))
  const user = userEvent.setup(); page()
  expect(await screen.findByText('HTTP 403')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry task' }))
  expect(await screen.findByRole('heading', { name: 'Specification' })).toBeTruthy()
 })
 it('distinguishes an absent exact Task from an unselected catalogue', async () => {
  vi.mocked(taskApi.get).mockResolvedValue(null)
  page()
  expect(await screen.findByText('Task not found')).toBeTruthy()
  expect(screen.queryByRole('article', { name: 'Task dossier' })).toBeNull()
 })
 it('aborts old-path section reads and ignores a late denial after a new dossier is selected', async () => {
  let reject!: (cause: Error) => void
  const pending = new Promise<TaskDetail>((_, fail) => { reject = fail })
  const mirror = { ...context, project_dir: '/mirror' }
  const current = { ...detail, task: { ...first, specification: { ...first.specification, title: 'New mirror dossier' } } }
  vi.mocked(taskApi.get).mockImplementation(async (selection, _, options) => options?.history ? pending : selection.project_dir === mirror.project_dir ? current : detail)
  const user = userEvent.setup()
  page(); await screen.findByRole('heading', { name: 'Specification' })
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  const signal = vi.mocked(taskApi.get).mock.calls.find(call => call[2]?.history)![2]!.signal!
  act(() => useTaskWorkspace.setState({ selection: mirror, contexts: [context, mirror] }))
  expect(await screen.findByRole('heading', { name: 'New mirror dossier' })).toBeTruthy()
  expect(signal.aborted).toBe(true)
  await act(async () => reject(new TaskReadError(403)))
  expect(useTaskWorkspace.getState().phase).toBe('ready')
  expect(screen.queryByRole('alert')).toBeNull()
  expect(screen.getByText('Lifecycle history has not been loaded.')).toBeTruthy()
 })
 it('invalidates the workspace when current authorization denies a complete project export', async () => {
  vi.mocked(taskApi.get).mockImplementation(async (_, __, options) => {
   if (options?.comments) throw new TaskReadError(403)
   return { ...detail, history: options?.history ? historyPage : null }
  })
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn() })
  const user = userEvent.setup()
  page(); await screen.findByRole('heading', { name: 'Specification' })
  await user.click(screen.getByRole('button', { name: 'Export all tasks' }))
  await waitFor(() => expect(useTaskWorkspace.getState().phase).toBe('forbidden'))
  expect(screen.queryByRole('article', { name: 'Task dossier' })).toBeNull()
  expect(URL.createObjectURL).not.toHaveBeenCalled()
 })
 it('loads full audit data only after the explicit all-task download', async () => {
  const user = userEvent.setup()
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: vi.fn(() => 'blob:tasks') })
  Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: vi.fn() })
  vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
  page(); await screen.findByRole('heading', { name: 'Specification' })
  expect(taskApi.get).toHaveBeenCalledTimes(1)
  await user.click(screen.getByRole('button', { name: 'Export all tasks' }))
  await waitFor(() => expect(URL.createObjectURL).toHaveBeenCalledOnce())
  expect(taskApi.get).toHaveBeenCalledWith(context, 'task2', { signal: expect.any(AbortSignal) })
  expect(URL.revokeObjectURL).toHaveBeenCalledWith('blob:tasks')
 })
  it('captures explicit version in JSON and prevents stale prior-project detail', async () => {
  page(); await screen.findByRole('heading', { name: 'Specification' })
  vi.mocked(taskApi.context).mockRejectedValueOnce(new Error('Ambiguous selected worktree'))
  act(() => useTaskWorkspace.setState({ selection: { ...context, project_id: 'other', project_version: 'feature' } }))
  expect(await screen.findByText('Ambiguous selected worktree')).toBeTruthy()
  expect(taskApi.context).toHaveBeenLastCalledWith({ ...context, project_id: 'other', project_version: 'feature' }, expect.any(AbortSignal))
   expect(screen.queryByRole('heading', { name: 'Specification' })).toBeNull()
  })
  it('preserves the literal mirror selection when resetting a dossier to an empty catalogue', async () => {
   const mirror = { ...context, project_dir: "/mirror" }
   vi.mocked(taskApi.catalogue).mockImplementation(async ({ selection }) => ({ results: stringifyNativeJson(selection) === stringifyNativeJson(mirror) ? [] : [summary(first)], next_cursor: null }))
   vi.mocked(taskApi.get).mockImplementation(async selection => stringifyNativeJson(selection) === stringifyNativeJson(mirror) ? null : detail)
   function SelectionLocation() { const location = useLocation(); return <output data-testid="selection-location">{location.pathname + location.search}</output> }
   render(<MemoryRouter initialEntries={[projectRecordHref('task', 'task1', context)]}><SelectionLocation /><Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes></MemoryRouter>)
   await screen.findByRole('heading', { name: 'Specification' })
   act(() => useTaskWorkspace.setState({ selection: mirror, contexts: [context, mirror], phase: 'ready' }))
   expect(await screen.findByText('No tasks match this view.')).toBeTruthy()
   expect(await screen.findByText('Select a task')).toBeTruthy()
   await waitFor(() => {
    expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ selection: mirror }))
    const uri = new URL(screen.getByTestId('selection-location').textContent!, 'http://fixture.invalid')
    expect(uri.pathname).toBe('/task/explorer')
    expect(parseNativeJson(uri.searchParams.get('selection') ?? 'null')).toEqual({ project_id: mirror.project_id, project_version: mirror.project_version })
   })
   expect(screen.queryByRole('article', { name: 'Task dossier' })).toBeNull()
   expect(screen.queryByRole('heading', { name: 'Record links' })).toBeNull()
  })
  it('retains an explicit native path and version when returning from a dossier', async () => {
   const user = userEvent.setup()
   const selected = { ...context, project_dir: "/mirror", project_version: 'feature/native' }
   useTaskWorkspace.setState({ selection: selected, contexts: [context, selected], phase: 'ready' })
   function SelectionLocation() { const location = useLocation(); return <output data-testid="selection-location">{location.pathname + location.search}</output> }
   render(<MemoryRouter initialEntries={[projectRecordHref('task', 'task1', selected)]}><SelectionLocation /><Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes></MemoryRouter>)
   await screen.findByRole('heading', { name: 'Specification' })
   await user.click(within(screen.getByRole('article', { name: 'Task dossier' })).getByRole('button', { name: 'Tasks' }))
   await waitFor(() => {
    const uri = new URL(screen.getByTestId('selection-location').textContent!, 'http://fixture.invalid')
    expect(uri.pathname).toBe('/task/explorer')
    expect(parseNativeJson(uri.searchParams.get('selection') ?? 'null')).toEqual({ project_id: selected.project_id, project_version: selected.project_version })
   })
   expect(screen.queryByRole('article', { name: 'Task dossier' })).toBeNull()
   expect(useTaskWorkspace.getState().selection).toEqual(selected)
  })
  it('rejects a late dossier from a different native path with the same project and version', async () => {
   let resolve!: (value: TaskDetail) => void
   const pending = new Promise<TaskDetail>(done => { resolve = done })
   const mirror = { ...context, project_dir: "/m" }
   const current = { ...detail, task: { ...first, specification: { ...first.specification, title: 'Current mirror task' } } }
   vi.mocked(taskApi.get).mockImplementationOnce(() => pending).mockResolvedValue(current)
   page()
   await waitFor(() => expect(taskApi.get).toHaveBeenCalledOnce())
   const oldSignal = vi.mocked(taskApi.get).mock.calls[0][2]!.signal!
   act(() => useTaskWorkspace.setState({ selection: mirror, contexts: [context, mirror] }))
   expect(await screen.findByRole('heading', { name: 'Current mirror task' })).toBeTruthy()
   expect(oldSignal.aborted).toBe(true)
   expect(taskApi.get).toHaveBeenLastCalledWith(mirror, 'task1', { signal: expect.any(AbortSignal) })
   await act(async () => resolve(detail))
   expect(screen.getByRole('heading', { name: 'Current mirror task' })).toBeTruthy()
  })
  it('aborts an unmounted dossier and ignores its late denial without invalidating the current context', async () => {
   let reject!: (cause: Error) => void
   const pending = new Promise<TaskDetail>((_, fail) => { reject = fail })
   vi.mocked(taskApi.get).mockImplementationOnce(() => pending)
   const view = page()
   await waitFor(() => expect(taskApi.get).toHaveBeenCalledOnce())
   const signal = vi.mocked(taskApi.get).mock.calls[0][2]!.signal!
   view.unmount()
   expect(signal.aborted).toBe(true)
   await act(async () => reject(new TaskReadError(403)))
   expect(useTaskWorkspace.getState().selection).toEqual(context)
   expect(useTaskWorkspace.getState().phase).toBe('ready')
  })
  it.each(['initial', 'loaded'] as const)('coordinates header refresh with a cloned context and a pending %s catalogue', async mode => {
   let resolve!: (value: TaskCataloguePage) => void
   const pending = new Promise<TaskCataloguePage>(done => { resolve = done })
   const value: TaskCataloguePage = { results: [summary(first)], next_cursor: null }
   const cloned = { ...context, project_dir: context.project_dir }
   vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [cloned], current: cloned })
   vi.mocked(taskApi.catalogue).mockImplementation(({ signal }) => new Promise((done, fail) => {
    void pending.then(done)
    signal?.addEventListener('abort', () => fail(new DOMException('Aborted', 'AbortError')), { once: true })
   }))
   if (mode === 'loaded') vi.mocked(taskApi.catalogue).mockResolvedValueOnce(value)
   render(<MemoryRouter initialEntries={['/task/explorer']}><WorkspaceRefreshProvider>
    <TaskWorkspaceSelectors /><TaskExplorerPage />
   </WorkspaceRefreshProvider></MemoryRouter>)
   if (mode === 'loaded') await screen.findByRole('heading', { name: 'Specification' })
   const refresh = within(screen.getByRole('group', { name: 'Working context' })).getByRole('button', { name: 'Refresh' }) as HTMLButtonElement
   fireEvent.click(refresh)
   const expected = mode === 'loaded' ? 2 : 1
   await waitFor(() => expect(taskApi.catalogue).toHaveBeenCalledTimes(expected))
   const signal = vi.mocked(taskApi.catalogue).mock.calls[expected - 1][0].signal!
   await act(async () => { await new Promise(done => window.setTimeout(done, 260)) })
   expect(taskApi.catalogue).toHaveBeenCalledTimes(expected)
   expect(signal.aborted).toBe(false)
   expect(refresh.disabled).toBe(true)
   await act(async () => resolve(value))
   await waitFor(() => expect(refresh.disabled).toBe(false))
  })
 })
