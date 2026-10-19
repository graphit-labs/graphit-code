import '@/test/contextControls'
import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Link, MemoryRouter, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { taskApi, TaskReadError, type TaskContexts } from '@/api/task'
import { taskContextKey, stopTaskWorkspace, useTaskWorkspace, invalidateTaskWorkspace } from './taskWorkspace'
import { nativeRecordContexts, NativeRecordReadError, type ProjectReadModule } from '@/api/nativeRecordRead'
import { TaskWorkspaceBootstrap, TaskWorkspaceNotice, TaskWorkspaceSelectors } from './TaskWorkspace'
import { useAppStore } from '@/store/appStore'
import { usePageRefresh } from '@/components/layout/WorkspaceRefresh'
import { WorkspaceRefreshProvider } from '@/components/layout/WorkspaceRefreshProvider'
import { context } from './task.test-fixtures'
import { projectRecordHref, requestedRecordSelection } from '@/api/nativeRecordNavigation'
import { TaskModeToggle } from './TaskModeToggle'
const mirror = { ...context, project_dir: "/q" }
vi.mock('@/api/task', async original => ({ ...await original<typeof import('@/api/task')>(), taskApi: { contexts: vi.fn() } }))
vi.mock('@/api/nativeRecordRead', async original => ({ ...await original<typeof import('@/api/nativeRecordRead')>(), nativeRecordContexts: vi.fn() }))
beforeEach(() => {
 stopTaskWorkspace(); vi.resetAllMocks()
 useAppStore.setState({ apiBase: '/api', webIdentity: { state: 'authenticated', provider: 'fixture', subject: 'reader' } })
})
function CurrentURI() { const location = useLocation(); return <output aria-label="Current record URI">{location.pathname + location.search}</output> }
describe('native retained working contexts', () => {
 it.each(['task', 'ast', 'knowledge', 'memory', 'now', 'hub'] as const)('renders the %s project name from its admitted inventory and retains the literal Unicode path', async module => {
  const selected = { ...context, project_dir: '/projetos/café Ω "quote\\ literal' }
  const inventory = { contexts: [selected], current: selected, projects: [{ project_id: context.project_id, name: 'Graphit Code · café Ω' }] }
  vi.mocked(taskApi.contexts).mockResolvedValue(inventory)
  vi.mocked(nativeRecordContexts).mockResolvedValue(inventory)
  await useTaskWorkspace.getState().refresh(undefined, module)
  render(<MemoryRouter><TaskWorkspaceSelectors module={module} /></MemoryRouter>)
  const project = screen.getByRole('combobox', { name: 'Project' })
  expect(project.textContent).toContain('Graphit Code · café Ω')
  expect(project.textContent).not.toContain(context.project_id)
  expect(project.getAttribute('title')).toBe(selected.project_dir)
  expect(screen.getByRole('combobox', { name: 'Version' }).textContent).toContain(selected.project_version)
  project.focus(); await userEvent.setup().keyboard('{Enter}')
  expect(screen.getByRole('option').textContent).toContain('Graphit Code · café Ω')
  expect(screen.getByRole('option').textContent).toContain(selected.project_dir)
 })
 it('refreshes display names without changing the exact selection and drops names absent from a later inventory', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [context], current: context, projects: [{ project_id: context.project_id, name: 'Original name' }, { project_id: 'not-admitted', name: 'Other name' }] })
  await useTaskWorkspace.getState().refresh()
  const selection = useTaskWorkspace.getState().selection
  expect(useTaskWorkspace.getState().projects).toEqual([{ project_id: context.project_id, name: 'Original name' }])
  render(<MemoryRouter><TaskWorkspaceSelectors /></MemoryRouter>)
  expect(screen.getByRole('combobox', { name: 'Project' }).textContent).toContain('Original name')
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [{ ...context }], current: { ...context }, projects: [{ project_id: context.project_id, name: 'Renamed project' }] })
  await act(() => useTaskWorkspace.getState().refresh())
  expect(useTaskWorkspace.getState().selection).toBe(selection)
  expect(screen.getByRole('combobox', { name: 'Project' }).textContent).toContain('Renamed project')
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [context], current: context })
  await act(() => useTaskWorkspace.getState().refresh())
  expect(screen.getByRole('combobox', { name: 'Project' }).textContent).toContain(context.project_id)
  expect(useTaskWorkspace.getState().projects).toEqual([])
 })
 it('discovers Memory and Knowledge through their own module even when Task is denied', async () => {
  vi.mocked(taskApi.contexts).mockRejectedValue(new TaskReadError(403))
  vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [mirror], current: mirror })
  for (const module of ['memory', 'knowledge', 'ast'] as const) {
   await useTaskWorkspace.getState().refresh(undefined, module)
   expect(nativeRecordContexts).toHaveBeenLastCalledWith(module, expect.any(AbortSignal))
   expect(useTaskWorkspace.getState()).toMatchObject({ module, selection: mirror, phase: 'ready' })
  }
  expect(taskApi.contexts).not.toHaveBeenCalled()
 })
 it('keeps a null admitted current instead of silently choosing an allowed sibling', async () => {
  vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [mirror], current: null })
  await useTaskWorkspace.getState().refresh(undefined, 'memory')
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [mirror], selection: null, phase: 'ready' })
  await useTaskWorkspace.getState().refresh(mirror, 'memory')
  expect(useTaskWorkspace.getState().selection).toEqual(mirror)
 })
 it('clears the old module immediately, aborts its discovery and fences late denial', async () => {
  let reject!: (cause: Error) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise((_done, fail) => { reject = fail }))
  useTaskWorkspace.setState({ selection: context, contexts: [context], phase: 'ready' })
  const old = useTaskWorkspace.getState().refresh()
  const oldSignal = vi.mocked(taskApi.contexts).mock.calls[0][0]!
  let finish!: (value: { contexts: typeof context[]; current: typeof context }) => void
  vi.mocked(nativeRecordContexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  const next = useTaskWorkspace.getState().refresh(undefined, 'memory')
  expect(useTaskWorkspace.getState()).toMatchObject({ module: 'memory', selection: null, contexts: [], phase: 'loading' })
  expect(oldSignal.aborted).toBe(true)
  finish({ contexts: [mirror], current: mirror }); await next
  reject(new TaskReadError(403)); await old
  invalidateTaskWorkspace(new TaskReadError(403), 'task')
  expect(useTaskWorkspace.getState()).toMatchObject({ module: 'memory', selection: mirror, phase: 'ready' })
  invalidateTaskWorkspace(new NativeRecordReadError(403), 'memory')
  expect(useTaskWorkspace.getState()).toMatchObject({ selection: null, phase: 'forbidden' })
 })
  it('validates an explicit reload selection against the captured inventory instead of choosing current', async () => {
   vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context, mirror], current: context })
   await useTaskWorkspace.getState().refresh(mirror)
   expect(useTaskWorkspace.getState().selection).toEqual(mirror)
   vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context], current: context })
   await expect(useTaskWorkspace.getState().refresh(mirror)).rejects.toThrow('selected working context is unavailable')
   expect(useTaskWorkspace.getState()).toMatchObject({ selection: null, contexts: [], phase: 'error' })
  })
 it('keeps exact NativePath and current, distinguishes the same project/version and selects by keyboard', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context, mirror], current: context, projects: [{ project_id: context.project_id, name: 'Graphit Code' }] })
  await useTaskWorkspace.getState().refresh()
  render(<MemoryRouter initialEntries={[projectRecordHref('task', 'literal task', context)]}><TaskWorkspaceSelectors /><CurrentURI /></MemoryRouter>)
  const user = userEvent.setup(); screen.getByRole('combobox', { name: 'Project' }).focus()
  await user.keyboard('{Enter}{ArrowDown}{Enter}')
  await waitFor(() => expect(taskContextKey(useTaskWorkspace.getState().selection)).toBe(taskContextKey(mirror)))
  expect(useTaskWorkspace.getState().selection?.project_dir).toEqual(mirror.project_dir)
  expect(screen.getByRole('combobox', { name: 'Project' }).textContent).toContain('Graphit Code')
  expect(taskContextKey(context)).not.toBe(taskContextKey(mirror))
  const uri = new URL(screen.getByLabelText('Current record URI').textContent!, window.location.origin)
  expect(requestedRecordSelection(uri.search)).toEqual(mirror)
  expect(Object.keys(requestedRecordSelection(uri.search)!)).toEqual(['project_dir', 'project_id', 'project_version'])
  expect(uri.pathname).toBe('/task/explorer/literal%20task')
 })
 it('preserves the complete selection while switching Task, Gantt and Session routes', async () => {
  render(<MemoryRouter initialEntries={[projectRecordHref('task', 'literal task', mirror)]}><TaskModeToggle mode="tasks" /><CurrentURI /></MemoryRouter>)
  const user = userEvent.setup()
  for (const [label, pathname] of [['Sessions', '/task/sessions'], ['Gantt', '/task/gantt'], ['Tasks', '/task/explorer']]) {
   await user.click(screen.getByText(label))
   const uri = new URL(screen.getByLabelText('Current record URI').textContent!, window.location.origin)
   expect(uri.pathname).toBe(pathname); expect(requestedRecordSelection(uri.search)).toEqual(mirror)
  }
 })
 it.each([[401, 'anonymous'], [403, 'forbidden'], [503, 'error']] as const)('clears prior rows on HTTP %s and retries the actual inventory', async (status, phase) => {
  useTaskWorkspace.setState({ selection: context, contexts: [context], projects: [{ project_id: context.project_id, name: 'Prior name' }], phase: 'ready' })
  vi.mocked(taskApi.contexts).mockRejectedValueOnce(new TaskReadError(status)).mockResolvedValueOnce({ contexts: [mirror], current: mirror })
  await expect(useTaskWorkspace.getState().refresh()).rejects.toThrow()
  expect(useTaskWorkspace.getState()).toMatchObject({ selection: null, contexts: [], projects: [], phase })
  render(<MemoryRouter><TaskWorkspaceNotice /></MemoryRouter>)
  if (status === 401) expect(screen.getByText('Sign in to read tasks')).toBeTruthy()
  else {
   await userEvent.setup().click(screen.getByRole('button', { name: 'Retry working contexts' }))
   await waitFor(() => expect(useTaskWorkspace.getState().selection).toEqual(mirror))
  }
 })
 it('aborts replaced inventory and prevents a late old scope from replacing the current one', async () => {
  let resolve!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { resolve = done })).mockResolvedValueOnce({ contexts: [mirror], current: mirror })
  const old = useTaskWorkspace.getState().refresh()
  const signal = vi.mocked(taskApi.contexts).mock.calls[0][0]
  await useTaskWorkspace.getState().refresh()
  await act(async () => { resolve({ contexts: [context], current: context }); await old })
  expect(signal?.aborted).toBe(true)
  expect(useTaskWorkspace.getState().selection).toEqual(mirror)
 })
 it('keeps header refresh busy until both current inventory and a registered reader settle', async () => {
  useTaskWorkspace.setState({ contexts: [context], selection: context, phase: 'ready' })
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context], current: context })
  let finish!: () => void
  const read = vi.fn(() => new Promise<void>(resolve => { finish = resolve }))
  function Reader() { usePageRefresh(read); return null }
  render(<MemoryRouter initialEntries={['/task/gantt']}><WorkspaceRefreshProvider><TaskWorkspaceSelectors /><Reader /></WorkspaceRefreshProvider></MemoryRouter>)
  await userEvent.setup().click(screen.getByRole('button', { name: 'Refresh' }))
  await waitFor(() => expect(read).toHaveBeenCalledTimes(1))
  expect(screen.getByRole('button', { name: 'Refresh' }).hasAttribute('disabled')).toBe(true)
  await act(async () => finish())
  await waitFor(() => expect(screen.getByRole('button', { name: 'Refresh' }).hasAttribute('disabled')).toBe(false))
 })
})

it.each(['ast', 'now'] as const)('does not retarget a withdrawn %s selection to an admitted sibling on refresh', async module => {
 vi.mocked(nativeRecordContexts).mockResolvedValueOnce({ contexts: [context], current: context }).mockResolvedValueOnce({ contexts: [mirror], current: mirror })
 await useTaskWorkspace.getState().refresh(undefined, module)
 await expect(useTaskWorkspace.getState().refresh(undefined, module)).rejects.toThrow('selected working context is unavailable')
 expect(useTaskWorkspace.getState()).toMatchObject({ module, contexts: [], selection: null, phase: 'error' })
})

function BootstrapFixture({ module = 'task', entry = '/task/explorer' }: { module?: ProjectReadModule; entry?: string }) {
 return <MemoryRouter initialEntries={[entry]}>
  <Link to="/task/explorer?page=second">Another page</Link>
  <Link to={projectRecordHref('session', 'session1', context)}>Original context</Link>
  <Link to={projectRecordHref('session', 'session1', mirror)}>Other known context</Link>
  <Link to="/task/sessions">Sessions without a selection parameter</Link>
  <TaskWorkspaceSelectors module={module} />
  <TaskWorkspaceBootstrap module={module}><input aria-label="Retained reader draft" defaultValue="Keep this draft" /></TaskWorkspaceBootstrap>
 </MemoryRouter>
}

describe('working-context bootstrap lifecycle', () => {
 it('revalidates the global preference with each module without carrying its admitted inventory', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context, mirror], current: context })
  const view = render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  await userEvent.setup().click(screen.getByText('Other known context'))
  await userEvent.setup().click(screen.getByText('Sessions without a selection parameter'))
  for (const module of ['ast', 'knowledge', 'memory', 'now', 'hub'] as const) {
   let finish!: (value: { contexts: typeof context[]; current: typeof context }) => void
   vi.mocked(nativeRecordContexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
   view.rerender(<BootstrapFixture module={module} />)
   expect(useTaskWorkspace.getState()).toMatchObject({ module, contexts: [], selection: null, preferredSelection: mirror, phase: 'loading' })
   expect(screen.queryByLabelText('Retained reader draft')).toBeNull()
   expect(nativeRecordContexts).toHaveBeenLastCalledWith(module, expect.any(AbortSignal))
   await act(async () => finish({ contexts: [context, mirror], current: context }))
   await screen.findByLabelText('Retained reader draft')
   expect(useTaskWorkspace.getState().selection).toEqual(mirror)
  }
  view.rerender(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  expect(useTaskWorkspace.getState().selection).toEqual(mirror)
  expect(taskApi.contexts).toHaveBeenCalledTimes(2); expect(nativeRecordContexts).toHaveBeenCalledTimes(5)
 })

 it('keeps an unavailable preference explicit instead of selecting another module current', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [mirror], current: mirror })
  const view = render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [context], current: context })
  view.rerender(<BootstrapFixture module="knowledge" />)
  await screen.findByText('Unable to read working contexts')
  expect(useTaskWorkspace.getState()).toMatchObject({ module: 'knowledge', contexts: [], selection: null, preferredSelection: mirror, phase: 'error' })
  expect(nativeRecordContexts).toHaveBeenCalledTimes(1)
 })

 it('lets an explicit URL override the preference after remount and validates it in the new module', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [mirror], current: mirror })
  const view = render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  view.unmount()
  vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [context, mirror], current: mirror })
  render(<BootstrapFixture module="ast" entry={projectRecordHref('session', 'session1', context)} />)
  await screen.findByLabelText('Retained reader draft')
  expect(useTaskWorkspace.getState()).toMatchObject({ module: 'ast', selection: context, preferredSelection: context, phase: 'ready' })
  expect(nativeRecordContexts).toHaveBeenCalledTimes(1)
 })

 it.each(['identity', 'apiBase'] as const)('discards the global preference when %s changes after discovery', async change => {
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [mirror], current: mirror }).mockResolvedValueOnce({ contexts: [context], current: context })
  render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  act(() => useAppStore.setState(change === 'identity' ? { webIdentity: { state: 'authenticated', provider: 'fixture', subject: 'another-reader' } } : { apiBase: '/another-api' }))
  await waitFor(() => expect(useTaskWorkspace.getState().selection).toEqual(context))
  expect(useTaskWorkspace.getState().preferredSelection).toEqual(context)
  expect(taskApi.contexts).toHaveBeenCalledTimes(2)
 })

 it('keeps pending discovery and its snapshot through query changes, known selections and same-module navigation', async () => {
  let finish!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  render(<BootstrapFixture />)
  const signal = vi.mocked(taskApi.contexts).mock.calls[0][0]!
  const user = userEvent.setup()
  await user.click(screen.getByText('Another page'))
  expect(taskApi.contexts).toHaveBeenCalledTimes(1); expect(signal.aborted).toBe(false)
  await act(async () => finish({ contexts: [context, mirror], current: context }))
  const draft = await screen.findByLabelText('Retained reader draft')
  await user.type(draft, ' literal')
  await user.click(screen.getByText('Other known context'))
  await waitFor(() => expect(useTaskWorkspace.getState().selection).toEqual(mirror))
  expect(screen.getByLabelText('Retained reader draft')).toBe(draft)
  await user.click(screen.getByText('Sessions without a selection parameter'))
  expect(useTaskWorkspace.getState().selection).toEqual(mirror)
  expect(screen.getByLabelText('Retained reader draft')).toHaveProperty('value', 'Keep this draft literal')
  expect(taskApi.contexts).toHaveBeenCalledTimes(1)
 })

 it('cancels an explicit inventory refresh when a known selection replaces it and ignores its late result', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [context, mirror], current: context })
  render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  let finish!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  let pending!: Promise<void>
  act(() => { pending = useTaskWorkspace.getState().refresh(context) })
  const signal = vi.mocked(taskApi.contexts).mock.calls[1][0]!
  await userEvent.setup().click(screen.getByText('Other known context'))
  expect(signal.aborted).toBe(true)
  await act(async () => { finish({ contexts: [context], current: context }); await pending })
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [context, mirror], selection: mirror, phase: 'ready', refreshing: false })
  expect(taskApi.contexts).toHaveBeenCalledTimes(2)
 })

 it.each([true, false])('revalidates an unknown URL selection once and accepts it only when discovered: %s', async available => {
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [context], current: context })
   .mockResolvedValueOnce({ contexts: available ? [context, mirror] : [context], current: context })
  render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  await userEvent.setup().click(screen.getByText('Other known context'))
  if (available) {
   await screen.findByLabelText('Retained reader draft')
   expect(useTaskWorkspace.getState().selection).toEqual(mirror)
  } else {
   await screen.findByText('Unable to read working contexts')
   expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [], selection: null, phase: 'error' })
  }
  expect(taskApi.contexts).toHaveBeenCalledTimes(2)
 })

 it('cancels unknown-selection discovery when returning to the already selected context', async () => {
  vi.mocked(taskApi.contexts).mockResolvedValueOnce({ contexts: [context], current: context })
  render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  let finish!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  const user = userEvent.setup()
  await user.click(screen.getByText('Other known context'))
  const signal = vi.mocked(taskApi.contexts).mock.calls[1][0]!
  await user.click(screen.getByText('Original context'))
  expect(signal.aborted).toBe(true)
  await screen.findByLabelText('Retained reader draft')
  await act(async () => finish({ contexts: [context, mirror], current: mirror }))
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [context], selection: context, phase: 'ready', refreshing: false })
  expect(taskApi.contexts).toHaveBeenCalledTimes(2)
 })

 it('rejects malformed selection JSON before discovery', async () => {
  render(<BootstrapFixture entry="/task/explorer?selection=%7Bbroken" />)
  await screen.findByText('Unable to read working contexts')
  expect(taskApi.contexts).not.toHaveBeenCalled()
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [], selection: null, phase: 'error' })
 })

 it.each(['identity', 'apiBase', 'module'] as const)('aborts prior discovery and refuses its late snapshot after a %s change', async change => {
  let finish!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
   .mockResolvedValue({ contexts: [mirror], current: mirror })
  vi.mocked(nativeRecordContexts).mockResolvedValue({ contexts: [mirror], current: mirror })
  const view = render(<BootstrapFixture />)
  const signal = vi.mocked(taskApi.contexts).mock.calls[0][0]!
  if (change === 'module') view.rerender(<BootstrapFixture module="memory" />)
  else act(() => useAppStore.setState(change === 'identity' ? { webIdentity: { state: 'authenticated', provider: 'fixture', subject: 'another-reader' } } : { apiBase: '/another-api' }))
  await screen.findByLabelText('Retained reader draft')
  expect(signal.aborted).toBe(true)
  await act(async () => finish({ contexts: [context], current: context }))
  expect(useTaskWorkspace.getState()).toMatchObject({ module: change === 'module' ? 'memory' : 'task', selection: mirror, contexts: [mirror], phase: 'ready' })
  expect(taskApi.contexts).toHaveBeenCalledTimes(change === 'module' ? 1 : 2)
  expect(nativeRecordContexts).toHaveBeenCalledTimes(change === 'module' ? 1 : 0)
 })

 it.each([401, 403])('keeps reader HTTP %s fail-closed instead of reusing the ready snapshot', async status => {
  vi.mocked(taskApi.contexts).mockResolvedValue({ contexts: [context, mirror], current: context })
  render(<BootstrapFixture />); await screen.findByLabelText('Retained reader draft')
  act(() => invalidateTaskWorkspace(new TaskReadError(status)))
  expect(screen.queryByLabelText('Retained reader draft')).toBeNull()
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [], selection: null, phase: status === 401 ? 'anonymous' : 'forbidden' })
  expect(taskApi.contexts).toHaveBeenCalledTimes(1)
 })

 it('aborts pending discovery on unmount without accepting a late completion', async () => {
  let finish!: (value: TaskContexts) => void
  vi.mocked(taskApi.contexts).mockImplementationOnce(() => new Promise(done => { finish = done }))
  const view = render(<BootstrapFixture />)
  const signal = vi.mocked(taskApi.contexts).mock.calls[0][0]!
  view.unmount(); expect(signal.aborted).toBe(true)
  await act(async () => finish({ contexts: [context], current: context }))
  expect(useTaskWorkspace.getState()).toMatchObject({ contexts: [], selection: null, phase: 'loading', refreshing: false })
 })
})
