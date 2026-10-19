import '@/test/contextControls'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi } from '@/api/task'
import { sessionApi } from '@/api/taskSession'
import { parseNativeJson, stringifyNativeJson } from '@/api/nativeJson'
import { projectRecordHref } from '@/api/nativeRecordNavigation'
import { useTaskWorkspace } from './taskWorkspace'
import { useAppStore } from '@/store/appStore'
import { TaskWorkspaceBootstrap } from './TaskWorkspace'
import SessionExplorerPage from './SessionExplorerPage'
import { context, cursor, first, second, sessionDetail, sessionHead, sessionHistoryPage, summary } from './task.test-fixtures'
vi.mock('@/api/task', async importOriginal => ({ ...await importOriginal<typeof import('@/api/task')>(), taskApi: { contexts: vi.fn(), context: vi.fn(), catalogue: vi.fn(), get: vi.fn() } }))
vi.mock('@/api/taskSession', async importOriginal => ({ ...await importOriginal<typeof import('@/api/taskSession')>(), sessionApi: { heads: vi.fn(), get: vi.fn() } }))
vi.mock('@/components/shared/RecordReferences', () => ({ RecordReferences: () => <h3>Record links</h3> }))
function Location() { return <output data-testid="location">{useLocation().pathname}</output> }
function page() { return render(<MemoryRouter initialEntries={['/task/sessions']}><Routes><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /><Route path="/task/explorer/:id?" element={<Location />} /></Routes></MemoryRouter>) }
beforeEach(() => {
 vi.stubGlobal('matchMedia', vi.fn((query: string) => ({ matches: false, media: query, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })))
 vi.clearAllMocks(); useAppStore.setState({ apiBase: '/api', webIdentity: { state: 'authenticated', provider: 'test', subject: 'first' } }); useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready', error: '' })
 vi.mocked(taskApi.context).mockImplementation(async selection => ({ ...context, ...selection }))
 vi.mocked(sessionApi.heads).mockResolvedValue([sessionHead, { ...sessionHead, id: 'session2', title: 'Completed session', status: 'completed' }])
 vi.mocked(sessionApi.get).mockResolvedValue(sessionDetail)
 vi.mocked(taskApi.catalogue).mockImplementation(async options => options.cursor ? { results: [summary(second)], next_cursor: null } : { results: [summary(first)], next_cursor: cursor })
})
describe('native Session Explorer', () => {
 it('renders exact native counts while exposing a bounded progress percentage', async () => {
  vi.mocked(sessionApi.heads).mockResolvedValue([{ ...sessionHead, completed_tasks: 18446744073709551614n, total_tasks: 18446744073709551615n }])
  vi.mocked(sessionApi.get).mockResolvedValue({ ...sessionDetail, session: { ...sessionDetail.session, revision: 9007199254740993n } })
  page(); await screen.findByRole('heading', { name: 'Request' })
  expect(screen.getByText('18446744073709551614 of 18446744073709551615 complete')).toBeTruthy()
  const progress = screen.getByRole('progressbar')
  expect(progress.getAttribute('aria-valuetext')).toBe('18446744073709551614 of 18446744073709551615 complete')
  expect(progress.getAttribute('aria-valuemax')).toBe('100')
  expect(Number(progress.getAttribute('aria-valuenow'))).toBeLessThanOrEqual(100)
  expect(screen.getByText('Current session response').parentElement?.querySelector('pre')?.textContent).toContain('"revision": 9007199254740993')
 })
 it('keeps cancelled tasks in the linked inventory without adding them to progress', async () => {
  vi.mocked(sessionApi.heads).mockResolvedValue([{ ...sessionHead, completed_tasks: 1, total_tasks: 2 }])
  const cancelled = { ...summary(second), id: 'cancelled-task', title: 'Cancelled task', status: 'cancelled' as const }
  vi.mocked(taskApi.catalogue).mockResolvedValue({ results: [{ ...summary(first), status: 'completed' }, summary(second), cancelled], next_cursor: null })
  page(); await screen.findByRole('link', { name: 'Cancelled task' })
  expect(screen.getByRole('progressbar', { name: 'Task progress: 1 of 2 complete' }).getAttribute('aria-valuenow')).toBe('50')
  expect(screen.getByRole('link', { name: 'Cancelled task' }).getAttribute('href')).toContain('/task/explorer/cancelled-task')
 })
 it('shows neutral progress when every linked task is cancelled', async () => {
  vi.mocked(sessionApi.heads).mockResolvedValue([{ ...sessionHead, completed_tasks: 0, total_tasks: 0 }])
  vi.mocked(taskApi.catalogue).mockResolvedValue({ results: [{ ...summary(first), status: 'cancelled' }], next_cursor: null })
  page(); await screen.findByRole('link', { name: 'First task' })
  expect(screen.getByText('No applicable tasks')).toBeTruthy()
  expect(screen.queryByRole('progressbar')).toBeNull()
 })
 it('reads current state on selection and leaves history unloaded until explicitly requested', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Request' })
  expect(screen.getByText('Session history has not been loaded.')).toBeTruthy()
  expect(screen.queryByText('No checkpoints on this page.')).toBeNull()
  expect(sessionApi.heads).toHaveBeenCalledWith(context, expect.any(AbortSignal))
  expect(sessionApi.get).toHaveBeenCalledWith(context, 'session1', { signal: expect.any(AbortSignal) })
  expect(screen.getAllByText('native UI').length).toBeGreaterThan(0)
  expect(screen.getAllByText('actual producer').length).toBeGreaterThan(0)
  expect(screen.getAllByText('Checkpoint').length).toBeGreaterThan(0)
  vi.mocked(sessionApi.get).mockResolvedValueOnce({ ...sessionDetail, history: sessionHistoryPage })
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  await screen.findByText('rev 4 · revised')
  expect(sessionApi.get).toHaveBeenLastCalledWith(context, 'session1', { signal: expect.any(AbortSignal), history: { page_size: 20, cursor: sessionDetail.history_start_cursor } })
  expect(screen.getByRole('heading', { name: 'Continuity' })).toBeTruthy()
  expect(screen.getAllByText(new Date(Number(sessionDetail.session.created_at)).toLocaleString()).length).toBeGreaterThan(0)
  expect(screen.getAllByText(new Date(Number(sessionDetail.session.ownership.lease_expires_at!)).toLocaleString()).length).toBeGreaterThan(0)
 })
 it('pages linked tasks using the actual session filter and keyboard navigation', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Request' })
  expect(taskApi.catalogue).toHaveBeenCalledWith(expect.objectContaining({ selection: context, filter: { session_id: 'session1' } }))
  await user.click(screen.getByRole('button', { name: 'Load more linked tasks' }))
  expect(await screen.findByRole('link', { name: 'Second task' })).toBeTruthy()
  expect(taskApi.catalogue).toHaveBeenLastCalledWith(expect.objectContaining({ cursor, filter: { session_id: 'session1' } }))
  screen.getByRole('link', { name: 'Second task' }).focus(); await user.keyboard('{Enter}')
  expect(screen.getByTestId('location').textContent).toBe('/task/explorer/task2')
 })
 it('filters the complete loaded title summaries honestly and retains active-only status', async () => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Request' })
  const calls = vi.mocked(sessionApi.heads).mock.calls.length
  await user.type(screen.getByLabelText('Search session titles'), 'Completed')
  expect(within(screen.getByLabelText('Session catalogue')).queryByText('First session')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Active only' }))
  expect(screen.getByText('No sessions match this view.')).toBeTruthy()
  expect(sessionApi.heads).toHaveBeenCalledTimes(calls)
 })
 it('waits for validated projects and rejects stale data or ambiguous physical selection', async () => {
  useTaskWorkspace.setState({ selection: null, contexts: [], phase: 'loading', error: '' }); page()
  expect(sessionApi.heads).not.toHaveBeenCalled()
  act(() => useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready', error: '' }))
  await screen.findByRole('heading', { name: 'Request' })
  vi.mocked(taskApi.context).mockRejectedValueOnce(new Error('Ambiguous physical selection'))
  act(() => useTaskWorkspace.setState({ selection: { ...context, project_id: 'other', project_version: 'feature' } }))
  expect(await screen.findByText('Ambiguous physical selection')).toBeTruthy()
  expect(screen.queryByRole('heading', { name: 'Request' })).toBeNull()
  expect(taskApi.context).toHaveBeenLastCalledWith({ ...context, project_id: 'other', project_version: 'feature' }, expect.any(AbortSignal))
 })
 it('preserves denial and recovers through explicit refresh', async () => {
  vi.mocked(sessionApi.heads).mockRejectedValueOnce(new Error('HTTP 403'))
  const user = userEvent.setup(); page()
  expect(await screen.findByText('HTTP 403')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry' }))
  await waitFor(() => expect(screen.getByRole('heading', { name: 'Request' })).toBeTruthy())
 })
  it('retains the exact NativePath when returning to Sessions, reloading and changing physical scope', async () => {
   const mirror = { ...context, project_dir: "/mÿr" }
   vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context, mirror], current: context })
   useTaskWorkspace.setState({ module: 'task', selection: null, contexts: [], phase: 'loading', error: '' })
   function SelectionLocation() { const location = useLocation(); return <output data-testid="selection-location">{location.pathname + location.search}</output> }
   const mount = (entry: string) => render(<MemoryRouter initialEntries={[entry]}><SelectionLocation /><TaskWorkspaceBootstrap><Routes><Route path="/task/sessions/:sessionId?" element={<SessionExplorerPage />} /></Routes></TaskWorkspaceBootstrap></MemoryRouter>)
   const user = userEvent.setup()
   const view = mount(projectRecordHref('session', 'session1', mirror))
   await screen.findByRole('article', { name: 'Session brief' })
   expect(sessionApi.get).toHaveBeenLastCalledWith(mirror, 'session1', { signal: expect.any(AbortSignal) })
   await user.click(within(screen.getByRole('article', { name: 'Session brief' })).getByRole('button', { name: 'Sessions' }))
   await waitFor(() => {
    const uri = new URL(screen.getByTestId('selection-location').textContent!, 'http://fixture.invalid')
    expect(uri.pathname).toBe('/task/sessions')
    expect(parseNativeJson(uri.searchParams.get('selection') ?? 'null')).toEqual(mirror)
    expect(useTaskWorkspace.getState().selection).toEqual(mirror)
   })
   const reload = screen.getByTestId('selection-location').textContent!
   expect(vi.mocked(sessionApi.heads).mock.calls.every(([selection]) => stringifyNativeJson(selection) === stringifyNativeJson(mirror))).toBe(true)
   view.unmount()
   expect(useTaskWorkspace.getState().selection).toBeNull()
   vi.mocked(sessionApi.heads).mockClear(); vi.mocked(sessionApi.get).mockClear()
   mount(reload)
   await screen.findByRole('article', { name: 'Session brief' })
   expect(sessionApi.heads).toHaveBeenLastCalledWith(mirror, expect.any(AbortSignal))
   expect(sessionApi.get).toHaveBeenLastCalledWith(mirror, 'session1', { signal: expect.any(AbortSignal) })
   expect(useTaskWorkspace.getState().selection).toEqual(mirror)
   act(() => useTaskWorkspace.setState({ selection: context }))
   await waitFor(() => {
    const uri = new URL(screen.getByTestId('selection-location').textContent!, 'http://fixture.invalid')
    expect(uri.pathname).toBe('/task/sessions')
    expect(parseNativeJson(uri.searchParams.get('selection') ?? 'null')).toEqual(context)
    expect(sessionApi.heads).toHaveBeenLastCalledWith(context, expect.any(AbortSignal))
   })
  })
 it('shows current state while linked tasks are pending and distinguishes a linked failure', async () => {
  let reject!: (cause: Error) => void
  vi.mocked(taskApi.catalogue).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail }))
  page(); await screen.findByRole('heading', { name: 'Request' })
  expect(screen.getByText('Loading linked tasks…')).toBeTruthy()
  expect(screen.queryByText('No tasks are linked to this session yet.')).toBeNull()
  await act(async () => reject(new Error('Linked catalogue unavailable')))
  expect(await screen.findByText('Linked catalogue unavailable')).toBeTruthy()
  expect(screen.getByRole('heading', { name: 'Request' })).toBeTruthy()
  const calls = vi.mocked(sessionApi.get).mock.calls.length
  await userEvent.setup().click(screen.getByRole('button', { name: 'Retry linked tasks' }))
  await screen.findByRole('link', { name: 'First task' })
  expect(sessionApi.get).toHaveBeenCalledTimes(calls)
 })
 it('distinguishes an absent Session from unloaded history', async () => {
  vi.mocked(sessionApi.get).mockResolvedValueOnce(null)
  page(); expect(await screen.findByText('Session unavailable')).toBeTruthy()
  expect(screen.queryByText('Session history has not been loaded.')).toBeNull()
  expect(screen.queryByRole('article', { name: 'Session brief' })).toBeNull()
 })
 it.each(['identity', 'API base'] as const)('aborts current and history on %s change and ignores late data or denial', async change => {
  const user = userEvent.setup(); page(); await screen.findByRole('heading', { name: 'Request' })
  let reject!: (cause: Error) => void
  vi.mocked(sessionApi.get).mockImplementationOnce(() => new Promise((_, fail) => { reject = fail }))
  await user.click(screen.getByRole('button', { name: 'Load history' }))
  const signal = vi.mocked(sessionApi.get).mock.calls.at(-1)![2]!.signal!
  act(() => useAppStore.setState(change === 'identity' ? { webIdentity: { state: 'authenticated', provider: 'test', subject: 'next' } } : { apiBase: '/other-api' }))
  expect(signal.aborted).toBe(true)
  await act(async () => reject(new Error('Private old history denial')))
  expect(screen.queryByText('Private old history denial')).toBeNull()
  await screen.findByRole('heading', { name: 'Request' })
  expect(screen.getByText('Session history has not been loaded.')).toBeTruthy()
 })

})
