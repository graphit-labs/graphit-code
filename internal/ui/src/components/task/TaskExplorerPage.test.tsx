import "@/test/contextControls"
import { WorkspaceSelectors } from "@/components/layout/WorkspaceSelectors"
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { taskApi, type Task, type TaskCatalogItem, type TaskExportDocument } from '@/api/task'
import { useAppStore } from '@/store/appStore'
import TaskExplorerPage from './TaskExplorerPage'

vi.mock('@/hooks/useToast', () => ({ showToast: vi.fn() }))
vi.mock('@/api/client', () => ({ api: { get: vi.fn(() => new Promise(() => {})) } }))
vi.mock('@/api/task', async importOriginal => {
  const actual = await importOriginal<typeof import('@/api/task')>()
  return { ...actual, taskApi: { list: vi.fn(), export: vi.fn() } }
})
vi.stubGlobal('matchMedia', vi.fn().mockImplementation(query => ({
  matches: false,
  media: query,
  onchange: null,
  addListener: vi.fn(),
  removeListener: vi.fn(),
  addEventListener: vi.fn(),
  removeEventListener: vi.fn(),
  dispatchEvent: vi.fn(),
})))

const first: Task = {
  id: 'tsk-aaaa', project_id: 'project-1', session_id: 'ses-aaaa', idempotency_key: 'first', title: 'First task',
  description: '# Objective\n\nBuild the **first deterministic feature**.\n\n- Preserve audit history\n- Render rich fields\n\nRun `go test` and inspect [the contract](https://example.com/contract).', type: 'feature', status: 'in_progress',
  priority: 1, checks: [], flagged: false, owner: 'agent-a', claim_epoch: 1,
  progress_sequence: 1, comment_sequence: 1, progress_summary: '**Core** landed with `go test ./internal/task`.', next_step: 'Verify the **Task Explorer** UI.',
  created_at: '2026-09-04T10:00:00Z', updated_at: '2026-09-04T11:00:00Z', revision: 4,
  ready: false,
}

const second: Task = {
  ...first, id: 'tsk-bbbb', idempotency_key: 'second', title: 'Second task',
  description: 'Resolve a blocked follow-up.', status: 'open', priority: 2, owner: undefined,
  claim_epoch: 0, progress_sequence: 0, comment_sequence: 0, progress_summary: undefined,
  next_step: undefined, blocked_by: ['tsk-aaaa'], revision: 1,
}

const completeExport: TaskExportDocument = {
  schema_version: 1,
  project_id: 'project-1',
  tasks: [first, second],
  dependencies: [{ key: 'tsk-bbbb/tsk-aaaa', task_id: 'tsk-bbbb', depends_on: 'tsk-aaaa', active: true, created_at: '2026-09-04T10:00:00Z', created_by: 'planner', revision: 1 }],
  checks: [{ key: 'tsk-aaaa/chk-1', task_id: 'tsk-aaaa', id: 'chk-1', kind: 'acceptance', text: 'The UI shows the **observable outcome**.', status: 'passed', evidence: 'Verified with `npm test`.', active: true, revision: 3 }],
  events: [{ key: 'tsk-aaaa/1', task_id: 'tsk-aaaa', sequence: 1, type: 'progress', actor: 'planner', at: '2026-09-04T10:00:00Z', summary: '**Implementation** completed.', next_step: 'Run `make test`.', revision: 1 }],
  comments: [{ id: 'cmt-1', task_id: 'tsk-aaaa', idempotency_key: 'decision', sequence: 1, kind: 'decision', body: 'Use one **canonical export**.', actor: 'agent-a', at: '2026-09-04T11:00:00Z', revision: 3 }],
  spec_revisions: [{
    key: 'tsk-aaaa/1', task_id: 'tsk-aaaa', source_revision: 2, kind: 'revised', actor: 'agent-a', reason: 'Clarified the **observable behavior**.', at: '2026-09-04T10:30:00Z',
    before: { title: 'First task', description: 'Initial **specification**.', type: 'feature', priority: 1, checks: [] },
    after: { title: 'First task', description: 'Revised **specification**.', type: 'feature', priority: 1, checks: [{ id: 'chk-1', kind: 'acceptance', text: 'The UI shows the **observable outcome**.', status: 'pending' }] },
  }],
}

const firstCatalogItem: TaskCatalogItem = {
  id: first.id, session_id: first.session_id, title: first.title, type: first.type, status: first.status,
  priority: first.priority, owner: first.owner, flagged: first.flagged,
  ready: first.ready, blocked_by: first.blocked_by, updated_at: first.updated_at,
}

const secondCatalogItem: TaskCatalogItem = {
  id: second.id, title: second.title, type: second.type, status: second.status,
  priority: second.priority, owner: second.owner, flagged: second.flagged,
  ready: second.ready, blocked_by: second.blocked_by, updated_at: second.updated_at,
}

function Location() {
  return <output data-testid="location">{useLocation().pathname}</output>
}

describe('Task Explorer', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    useAppStore.setState({ activeProjectKey: 'workspace:demo:/project', activeProjectOrigin: 'workspace', activeProjectId: 'demo', activeProjectDir: '/project', projectName: 'Demo', projects: [], projectsLoaded: false })
    vi.mocked(taskApi.list).mockImplementation(async options => options.cursor
      ? { results: [secondCatalogItem], next_cursor: '' }
      : { results: [firstCatalogItem], next_cursor: 'page-2' })
    vi.mocked(taskApi.export).mockImplementation(async (_projectDir, id) => {
      if (!id) return completeExport
      return {
        ...completeExport,
        task_id: id,
        tasks: completeExport.tasks.filter(task => task.id === id),
        dependencies: completeExport.dependencies.filter(item => item.task_id === id),
        events: completeExport.events.filter(item => item.task_id === id),
        comments: completeExport.comments.filter(item => item.task_id === id),
      }
    })
  })

  it('switches Workspace to Hub and back without reusing either request scope', async () => {
    useAppStore.setState({
      activeProjectKey: 'workspace:local:/project', activeProjectOrigin: 'workspace',
      activeProjectId: 'local', activeProjectDir: '/project', projectName: 'Local',
    })
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )
    await waitFor(() => expect(taskApi.list).toHaveBeenCalledWith(expect.objectContaining({ projectDir: '/project' })))

    act(() => useAppStore.setState({
      activeProjectKey: 'hub:01HUB', activeProjectOrigin: 'hub', activeProjectId: '01HUB',
      activeProjectDir: '', projectName: 'Remote',
    }))
    await waitFor(() => expect(taskApi.list).toHaveBeenCalledWith(expect.objectContaining({ projectDir: undefined, projectId: '01HUB' })))
    await waitFor(() => expect(taskApi.export).toHaveBeenCalledWith(undefined, 'tsk-aaaa', '01HUB'))

    act(() => useAppStore.setState({
      activeProjectKey: 'workspace:other:/other', activeProjectOrigin: 'workspace', activeProjectId: 'other',
      activeProjectDir: '/other', projectName: 'Other',
    }))
    await waitFor(() => expect(taskApi.list).toHaveBeenCalledWith(expect.objectContaining({ projectDir: '/other' })))
    expect(taskApi.list).not.toHaveBeenCalledWith(expect.objectContaining({ projectDir: '/other', projectId: '01HUB' }))
  })

  it('loads a bounded catalogue, renders exact detail, and appends the next page', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/explorer/:taskId?" element={<><TaskExplorerPage /><Location /></>} />
        </Routes>
      </MemoryRouter>,
    )

    expect(await screen.findByRole('heading', { name: 'Specification' })).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Objective' })).toBeTruthy()
    expect(screen.getByText('first deterministic feature')).toBeTruthy()
    expect(screen.getByText('canonical export')).toBeTruthy()
    expect(screen.getAllByText('observable outcome').length).toBeGreaterThan(0)
    expect(screen.getByText('npm test')).toBeTruthy()
    expect(screen.getByText('make test')).toBeTruthy()
    const specification = screen.getByRole('heading', { name: 'Specification' }).closest('section')
    expect(specification).not.toBeNull()
    expect(within(specification!).queryByText(/# Objective/)).toBeNull()
    expect(within(specification!).getAllByRole('listitem').map(item => item.textContent)).toEqual(['Preserve audit history', 'Render rich fields'])
    expect(within(specification!).getByText('go test').tagName).toBe('CODE')
    expect(within(specification!).getByRole('link', { name: 'the contract' }).getAttribute('href')).toBe('https://example.com/contract')
    const accountability = screen.getByRole('heading', { name: 'Accountability' })
    const openSession = screen.getByRole('button', { name: 'Open session' })
    const recordLinks = screen.getByRole('heading', { name: 'Record links' })
    expect(accountability.compareDocumentPosition(recordLinks) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(openSession.compareDocumentPosition(recordLinks) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(screen.getByText('Resolving references…').getAttribute('role')).toBe('status')
    expect(taskApi.list).toHaveBeenCalledWith({ projectDir: '/project', query: undefined, status: 'all', pageSize: 20, cursor: undefined })
    expect(taskApi.export).toHaveBeenCalledWith('/project', 'tsk-aaaa')
    expect(taskApi.export).not.toHaveBeenCalledWith('/project')

    const taskList = screen.getByLabelText('Task catalogue')
    await user.click(within(taskList).getByRole('button', { name: 'Load more' }))
    await waitFor(() => expect(taskApi.list).toHaveBeenCalledWith({ projectDir: '/project', query: undefined, status: 'all', pageSize: 20, cursor: 'page-2' }))
    expect(await within(taskList).findByText('Second task')).toBeTruthy()

    await user.click(within(taskList).getByText('Second task'))
    await waitFor(() => expect(taskApi.export).toHaveBeenCalledWith('/project', 'tsk-bbbb'))
    expect(screen.getByTestId('location').textContent).toBe('/task/explorer/tsk-bbbb')
    expect(await screen.findByText('Resolve a blocked follow-up.')).toBeTruthy()
  })

  it('renders Markdown in current and historical rich-text fields', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    expect(await screen.findByText('Core')).toBeTruthy()
    expect(screen.getByText('Task Explorer')).toBeTruthy()
    expect(screen.getByText('Implementation')).toBeTruthy()
    expect(screen.getByText('observable behavior')).toBeTruthy()

    await user.click(screen.getByText('rev 2 · revised'))
    expect(screen.getByText('Before').parentElement?.textContent).toContain('Initial specification.')
    expect(screen.getByText('After').parentElement?.textContent).toContain('Revised specification.')
    expect(screen.getAllByText('observable outcome').length).toBeGreaterThan(1)
  })

  it('uses the custom status selector and sends search and status to the catalogue API', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    const selector = screen.getByRole('combobox', { name: 'Filter task status' })
    await user.click(selector)
    const options = screen.getByRole('listbox', { name: 'Task statuses' })
    await user.click(within(options).getByRole('option', { name: 'Blocked' }))
    await user.type(screen.getByLabelText('Search tasks'), 'scheduler')

    await waitFor(() => expect(taskApi.list).toHaveBeenLastCalledWith({
      projectDir: '/project', query: 'scheduler', status: 'blocked', pageSize: 20, cursor: undefined,
    }))
    expect(screen.queryByRole('listbox', { name: 'Task statuses' })).toBeNull()
  })

  it('shows a session chip for a task linked to a session and navigates to it', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} />
          <Route path="/task/sessions/:sessionId" element={<Location />} />
        </Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    const sessionChips = screen.getAllByTitle('Open linked session')
    expect(sessionChips.length).toBeGreaterThan(0)
    await user.click(sessionChips[0])
    expect(screen.getByTestId('location').textContent).toBe('/task/sessions/ses-aaaa')
  })

  it('switches to the sessions view via the mode toggle', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} />
          <Route path="/task/sessions" element={<Location />} />
        </Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    await user.click(screen.getByRole('tab', { name: 'Sessions' }))
    expect(screen.getByTestId('location').textContent).toBe('/task/sessions')
  })

  it('requests the complete all-task export only after explicit download', async () => {
    const user = userEvent.setup()
    const createObjectURL = vi.fn(() => 'blob:task-export')
    const revokeObjectURL = vi.fn()
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => {})
    Object.defineProperty(URL, 'createObjectURL', { configurable: true, value: createObjectURL })
    Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, value: revokeObjectURL })
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    expect(taskApi.export).not.toHaveBeenCalledWith('/project')
    await user.click(screen.getByRole('button', { name: 'Export all tasks' }))
    await waitFor(() => expect(taskApi.export).toHaveBeenCalledWith('/project'))
    expect(createObjectURL).toHaveBeenCalledOnce()
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:task-export')
    click.mockRestore()
  })

  it('keeps the detail rendered when the selected task row is clicked again', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes>
          <Route path="/task/explorer/:taskId?" element={<><TaskExplorerPage /><Location /></>} />
        </Routes>
      </MemoryRouter>,
    )

    expect(await screen.findByRole('heading', { name: 'Specification' })).toBeTruthy()
    const taskList = screen.getByLabelText('Task catalogue')
    const loaded = vi.mocked(taskApi.export).mock.calls.length

    await user.click(within(taskList).getByText('First task'))

    expect(screen.getByRole('heading', { name: 'Specification' })).toBeTruthy()
    expect(screen.queryByText('Select a task')).toBeNull()
    expect(vi.mocked(taskApi.export).mock.calls.length).toBe(loaded)
    expect(screen.getByTestId('location').textContent).toBe('/task/explorer/tsk-aaaa')
  })

  it('retries the detail request when the selected task failed to load', async () => {
    const user = userEvent.setup()
    vi.mocked(taskApi.export).mockRejectedValueOnce(new Error('unavailable'))
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    const taskList = await screen.findByLabelText('Task catalogue')
    await within(taskList).findByText('First task')
    await waitFor(() => expect(taskApi.export).toHaveBeenCalledWith('/project', 'tsk-aaaa'))
    expect(await screen.findByText('Select a task')).toBeTruthy()

    await user.click(within(taskList).getByText('First task'))

    expect(await screen.findByRole('heading', { name: 'Specification' })).toBeTruthy()
    expect(vi.mocked(taskApi.export).mock.calls.filter(([, id]) => id === 'tsk-aaaa').length).toBe(2)
  })

  it('switches the active project from the shared header', async () => {
    const user = userEvent.setup()
    useAppStore.setState({
      projects: [
        { id: 'p1', name: 'Demo', dir: '/project' },
        { id: 'p2', name: 'Other', dir: '/other' },
      ] as never,
      projectsLoaded: true,
    })
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    const picker = screen.getByRole('combobox', { name: 'Project' })
    expect(picker.textContent).toContain('Demo')
    await user.click(picker)
    await user.click(screen.getByRole('option', { name: 'Other' }))

    expect(useAppStore.getState().activeProjectDir).toBe('/other')
    await waitFor(() => expect(taskApi.list).toHaveBeenLastCalledWith({
      projectDir: '/other', query: undefined, status: 'all', pageSize: 20, cursor: undefined,
    }))
    expect(screen.getAllByRole('combobox', { name: 'Project' })).toHaveLength(1)
  })

  it('keeps the global project control disabled while projects load', async () => {
    useAppStore.setState({ projects: [], projectsLoaded: false })
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <header><WorkspaceSelectors /></header>
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    expect((screen.getByRole('combobox', { name: 'Project' }) as HTMLSelectElement).disabled).toBe(true)
  })
})
