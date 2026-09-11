import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { taskApi, type Task, type TaskCatalogItem, type TaskExportDocument } from '@/api/task'
import { useAppStore } from '@/store/appStore'
import TaskExplorerPage from './TaskExplorerPage'

vi.mock('@/hooks/useToast', () => ({ showToast: vi.fn() }))
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
  id: 'tsk-aaaa', project_id: 'project-1', idempotency_key: 'first', title: 'First task',
  description: '# Objective\n\nBuild the **first deterministic feature**.\n\n- Preserve audit history\n- Render rich fields', type: 'feature', status: 'in_progress',
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
  id: first.id, title: first.title, type: first.type, status: first.status,
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
    useAppStore.setState({ activeProjectDir: '/project', projectName: 'Demo' })
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

  it('loads a bounded catalogue, renders exact detail, and appends the next page', async () => {
    const user = userEvent.setup()
    render(
      <MemoryRouter initialEntries={['/task/explorer']}>
        <Routes>
          <Route path="/task/explorer/:taskId?" element={<><TaskExplorerPage /><Location /></>} />
        </Routes>
      </MemoryRouter>,
    )

    expect(await screen.findByText('Specification')).toBeTruthy()
    expect(screen.getByRole('heading', { name: 'Objective' })).toBeTruthy()
    expect(screen.getByText('first deterministic feature')).toBeTruthy()
    expect(screen.getByText('canonical export')).toBeTruthy()
    expect(screen.getAllByText('observable outcome').length).toBeGreaterThan(0)
    expect(screen.getByText('npm test')).toBeTruthy()
    expect(screen.getByText('make test')).toBeTruthy()
    const specification = screen.getByText('Specification').closest('section')
    expect(specification).not.toBeNull()
    expect(within(specification!).queryByText(/# Objective/)).toBeNull()
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
        <Routes><Route path="/task/explorer/:taskId?" element={<TaskExplorerPage />} /></Routes>
      </MemoryRouter>,
    )

    await screen.findByText('First task')
    const selector = screen.getByRole('button', { name: 'Filter task status' })
    await user.click(selector)
    const options = screen.getByRole('listbox', { name: 'Task statuses' })
    await user.click(within(options).getByRole('option', { name: 'Blocked' }))
    await user.type(screen.getByLabelText('Search tasks'), 'scheduler')

    await waitFor(() => expect(taskApi.list).toHaveBeenLastCalledWith({
      projectDir: '/project', query: 'scheduler', status: 'blocked', pageSize: 20, cursor: undefined,
    }))
    expect(screen.queryByRole('listbox', { name: 'Task statuses' })).toBeNull()
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
})
